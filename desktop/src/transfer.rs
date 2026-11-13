//! Files to and from the phone, over the network.
//!
//! The Bluetooth link arranges each transfer and this carries it; the wire
//! format is described in `omalink_proto::transfer`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use omalink_proto::transfer::{CHUNK, TAG};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc::UnboundedSender;

use crate::core::Event;

/// Where the phone is listening for one file.
pub struct Route {
    pub addrs: Vec<String>,
    pub port: u16,
    pub key: Vec<u8>,
}

/// Which transfer an event is about: ids from the two sides can coincide.
pub fn key(incoming: bool, id: u64) -> String {
    format!("{}{id}", if incoming { "in" } else { "out" })
}

async fn connect(route: &Route) -> Result<TcpStream, String> {
    for addr in &route.addrs {
        // A phone has addresses the desktop can't reach, such as its mobile
        // data one; don't wait long on each.
        if let Ok(Ok(stream)) = tokio::time::timeout(Duration::from_secs(3), TcpStream::connect((addr.as_str(), route.port))).await {
            let _ = stream.set_nodelay(true);
            return Ok(stream);
        }
    }
    Err("Couldn't reach the phone over the network. Put both on the same Wi-Fi.".into())
}

fn cipher(route: &Route) -> Result<Aes256Gcm, String> {
    Aes256Gcm::new_from_slice(&route.key).map_err(|_| "The phone sent an unusable key.".to_string())
}

fn nonce(index: u64) -> [u8; 12] {
    let mut bytes = [0u8; 12];
    bytes[4..].copy_from_slice(&index.to_be_bytes());
    bytes
}

/// Reports progress now and then rather than for every record.
struct Meter {
    tx: UnboundedSender<Event>,
    key: String,
    done: u64,
    told: u64,
}

impl Meter {
    fn add(&mut self, bytes: usize) {
        self.done += bytes as u64;
        if self.done - self.told >= 512 * 1024 {
            self.told = self.done;
            let _ = self.tx.send(Event::TransferProgress { key: self.key.clone(), done: self.done });
        }
    }
}

async fn receive(route: Route, dest: &Path, meter: &mut Meter) -> Result<(), String> {
    let cipher = cipher(&route)?;
    let mut stream = connect(&route).await?;
    let mut file = File::create(dest).await.map_err(|e| format!("Couldn't write {}: {e}", dest.display()))?;
    let cut = |_| "The connection to the phone was lost.".to_string();
    let mut record = vec![0u8; CHUNK + TAG];
    for index in 0u64.. {
        let length = stream.read_u32().await.map_err(cut)? as usize;
        if !(TAG..=CHUNK + TAG).contains(&length) {
            return Err("The phone sent something unreadable.".into());
        }
        stream.read_exact(&mut record[..length]).await.map_err(cut)?;
        let plain = cipher
            .decrypt(Nonce::from_slice(&nonce(index)), &record[..length])
            .map_err(|_| "The file didn't arrive intact.".to_string())?;
        if plain.is_empty() {
            break;
        }
        file.write_all(&plain).await.map_err(|e| format!("Couldn't write the file: {e}"))?;
        meter.add(plain.len());
    }
    file.flush().await.map_err(|e| format!("Couldn't write the file: {e}"))
}

async fn send(route: Route, source: &Path, meter: &mut Meter) -> Result<(), String> {
    let cipher = cipher(&route)?;
    let mut file = File::open(source).await.map_err(|e| format!("Couldn't read {}: {e}", source.display()))?;
    let mut stream = connect(&route).await?;
    let cut = |_| "The connection to the phone was lost.".to_string();
    let mut chunk = vec![0u8; CHUNK];
    for index in 0u64.. {
        // Fill the chunk; a short read is not the end of the file.
        let mut filled = 0;
        while filled < CHUNK {
            match file.read(&mut chunk[filled..]).await.map_err(|e| format!("Couldn't read the file: {e}"))? {
                0 => break,
                n => filled += n,
            }
        }
        let record = cipher
            .encrypt(Nonce::from_slice(&nonce(index)), &chunk[..filled])
            .map_err(|_| "Couldn't encrypt the file.".to_string())?;
        stream.write_u32(record.len() as u32).await.map_err(cut)?;
        stream.write_all(&record).await.map_err(cut)?;
        if filled == 0 {
            break;
        }
        meter.add(filled);
    }
    stream.flush().await.map_err(cut)?;
    // The phone closes once it has the closing record.
    let _ = tokio::time::timeout(Duration::from_secs(10), stream.read_u8()).await;
    Ok(())
}

/// Fetches a file the phone offered into `dest`, by way of a `.part` file so
/// a half-arrived file never carries the real name.
pub async fn download(route: Route, dest: PathBuf, id: u64, tx: UnboundedSender<Event>) {
    let key = key(true, id);
    let mut meter = Meter { tx: tx.clone(), key: key.clone(), done: 0, told: 0 };
    let mut part = dest.clone().into_os_string();
    part.push(".part");
    let part = PathBuf::from(part);
    let mut result = receive(route, &part, &mut meter).await;
    if result.is_ok() {
        result = tokio::fs::rename(&part, &dest).await.map_err(|e| format!("Couldn't save the file: {e}"));
    } else {
        let _ = tokio::fs::remove_file(&part).await;
    }
    let _ = tx.send(Event::TransferEnd { key, done: meter.done, error: result.err() });
}

pub async fn upload(route: Route, source: PathBuf, id: u64, tx: UnboundedSender<Event>) {
    let key = key(false, id);
    let mut meter = Meter { tx: tx.clone(), key: key.clone(), done: 0, told: 0 };
    let result = send(route, &source, &mut meter).await;
    let _ = tx.send(Event::TransferEnd { key, done: meter.done, error: result.err() });
}

/// `~/Downloads/name`, or `name (2)` and so on when that is taken.
pub fn destination(name: &str) -> PathBuf {
    let dir = std::env::var("XDG_DOWNLOAD_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Downloads"));
    let _ = std::fs::create_dir_all(&dir);
    // Only the last component: the name comes from another device.
    let name = Path::new(name).file_name().and_then(|n| n.to_str()).filter(|n| !n.starts_with('.')).unwrap_or("file");
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem, format!(".{extension}")),
        _ => (name, String::new()),
    };
    let mut path = dir.join(name);
    let mut n = 2;
    while path.exists() || path.with_file_name(format!("{}.part", path.file_name().unwrap_or_default().to_string_lossy())).exists() {
        path = dir.join(format!("{stem} ({n}){extension}"));
        n += 1;
    }
    path
}
