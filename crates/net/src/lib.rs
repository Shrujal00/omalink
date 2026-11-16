//! The network link between the desktop and the phone, shared by both apps.
//!
//! Each device has one long-term key; its public half is its address. iroh
//! finds a way between two addresses: straight across the local network,
//! straight to the other side's public IP (the desktop asks its router to
//! open a port through UPnP, NAT-PMP or PCP), by punching through NAT, or,
//! when nothing else works, through a relay that only ever sees encrypted
//! bytes. The wire format on top is in `omalink_proto::net`.
//!
//! Nothing here decides who may connect: that is the app's, from the keys it
//! has learned to trust through icon verification.

use std::path::{Path, PathBuf};
use std::time::Duration;

use iroh::endpoint::{presets, Connection, RecvStream, SendStream};
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayMap, RelayMode, RelayUrl, SecretKey};
use iroh_mdns_address_lookup::{DiscoveryEvent, MdnsAddressLookup};
use n0_future::{Stream, StreamExt};
use omalink_proto::net::{LINK_ALPN, MDNS_SERVICE, PAIR_ALPN};
use omalink_proto::{encode, Codec, DecodeError, Message, MAX_FRAME};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub use iroh;
pub use iroh::endpoint::Connection as Conn;

/// Which app this is; the desktop is the one that shows icons and that the
/// phone looks for on the local network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Desktop,
    Phone,
}

/// A computer on the local network that is ready to pair right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    Open { id: String, name: String },
    Gone { id: String },
}

pub struct Node {
    pub endpoint: Endpoint,
    mdns: MdnsAddressLookup,
    role: Role,
}

/// What the other side's address looks like on the wire.
pub fn id_string(id: &EndpointId) -> String {
    id.to_string()
}

pub fn parse_id(text: &str) -> Result<EndpointId, String> {
    text.trim().parse::<EndpointId>().map_err(|_| "That isn't a device address.".to_string())
}

/// The key bytes the icon is derived from.
pub fn id_bytes(id: &EndpointId) -> [u8; 32] {
    *id.as_bytes()
}

/// 32 random bytes, for keys and verification nonces.
pub fn random32() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the system has a random number source");
    bytes
}

/// This device's secret key, made on first use and kept readable by its
/// owner only.
pub fn load_key(path: &Path) -> Result<[u8; 32], String> {
    if let Ok(bytes) = std::fs::read(path) {
        if let Ok(key) = <[u8; 32]>::try_from(bytes.as_slice()) {
            return Ok(key);
        }
    }
    let key = SecretKey::generate().to_bytes();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Couldn't create {}: {e}", dir.display()))?;
    }
    let temp = path.with_extension("tmp");
    write_private(&temp, &key)?;
    std::fs::rename(&temp, path).map_err(|e| format!("Couldn't save the device key: {e}"))?;
    Ok(key)
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| format!("Couldn't save the device key: {e}"))?;
    file.write_all(bytes).map_err(|e| format!("Couldn't save the device key: {e}"))
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("Couldn't save the device key: {e}"))
}

/// A device this one has verified with icons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Peer {
    pub id: String,
    pub name: String,
    /// Milliseconds since the Unix epoch.
    pub added: i64,
    /// Where it was last reached, tried before looking it up afresh.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub addr: Option<EndpointAddr>,
}

/// The devices this one trusts, kept in one JSON file.
#[derive(Debug, Default)]
pub struct Trust {
    path: PathBuf,
    pub peers: Vec<Peer>,
}

impl Trust {
    pub fn load(path: PathBuf) -> Self {
        let peers = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { path, peers }
    }

    pub fn contains(&self, id: &str) -> bool {
        self.peers.iter().any(|p| p.id == id)
    }

    pub fn name(&self, id: &str) -> Option<&str> {
        self.peers.iter().find(|p| p.id == id).map(|p| p.name.as_str())
    }

    pub fn add(&mut self, peer: Peer) {
        self.peers.retain(|p| p.id != peer.id);
        self.peers.push(peer);
        self.save();
    }

    /// Remembers where a trusted device was just reached.
    pub fn update_addr(&mut self, id: &str, addr: EndpointAddr) {
        if let Some(peer) = self.peers.iter_mut().find(|p| p.id == id) {
            if peer.addr.as_ref() != Some(&addr) {
                peer.addr = Some(addr);
                self.save();
            }
        }
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.peers.len();
        self.peers.retain(|p| p.id != id);
        let changed = self.peers.len() != before;
        if changed {
            self.save();
        }
        changed
    }

    fn save(&self) {
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let temp = self.path.with_extension("json.tmp");
        if write_private(&temp, &serde_json::to_vec_pretty(&self.peers).unwrap_or_default()).is_ok() {
            let _ = std::fs::rename(temp, &self.path);
        }
    }
}

/// Where to relay when no direct path works: the public relays, or one of
/// the person's own (`iroh-relay`), given as a URL.
pub fn relay_mode(relay: &str) -> Result<RelayMode, String> {
    let relay = relay.trim();
    if relay.is_empty() {
        return Ok(RelayMode::Default);
    }
    let url: RelayUrl = relay.parse().map_err(|_| format!("{relay} isn't a relay address."))?;
    Ok(RelayMode::Custom(RelayMap::from(url)))
}

impl Node {
    pub async fn start(secret: [u8; 32], role: Role, relay: &str) -> Result<Self, String> {
        Self::start_with(secret, role, relay_mode(relay)?).await
    }

    pub async fn start_with(secret: [u8; 32], role: Role, relay: RelayMode) -> Result<Self, String> {
        let endpoint = Endpoint::builder(presets::N0)
            .secret_key(SecretKey::from_bytes(&secret))
            .alpns(vec![PAIR_ALPN.to_vec(), LINK_ALPN.to_vec()])
            .relay_mode(relay)
            .bind()
            .await
            .map_err(|e| format!("Couldn't start networking: {e}"))?;
        let mdns = MdnsAddressLookup::builder()
            .service_name(MDNS_SERVICE)
            .build(endpoint.id())
            .map_err(|e| format!("Couldn't use the local network: {e}"))?;
        endpoint
            .address_lookup()
            .map_err(|e| format!("Couldn't use the local network: {e}"))?
            .add(mdns.clone());
        let node = Self { endpoint, mdns, role };
        node.set_pairing(None);
        Ok(node)
    }

    pub fn id(&self) -> String {
        id_string(&self.endpoint.id())
    }

    /// While the desktop is pairing it puts its name in what it advertises,
    /// so phones nearby can list it; otherwise only which app it is. The
    /// advertisement also reaches iroh's address lookup, so names aren't
    /// left in it longer than pairing takes.
    pub fn set_pairing(&self, name: Option<&str>) {
        let marker = match (self.role, name) {
            (Role::Desktop, Some(name)) => {
                let mut text = format!("d:{name}");
                // On a character boundary, within the advertisement's limit.
                while text.len() > 200 {
                    text.pop();
                }
                text
            }
            (Role::Desktop, None) => "d".to_string(),
            (Role::Phone, _) => "p".to_string(),
        };
        self.endpoint.set_user_data_for_address_lookup(marker.try_into().ok());
    }

    /// Computers on the local network as they open and close pairing.
    pub async fn browse(&self) -> impl Stream<Item = Found> + Unpin + use<> {
        self.mdns.subscribe().await.filter_map(|event| match event {
            DiscoveryEvent::Discovered { endpoint_info, .. } => {
                let id = id_string(&endpoint_info.endpoint_id);
                match endpoint_info.data.user_data().map(|d| d.to_string()) {
                    Some(data) if data.starts_with("d:") => Some(Found::Open { id, name: data[2..].to_string() }),
                    Some(data) if data == "d" => Some(Found::Gone { id }),
                    _ => None,
                }
            }
            DiscoveryEvent::Expired { endpoint_id } => Some(Found::Gone { id: id_string(&endpoint_id) }),
            _ => None,
        })
    }

    /// The next incoming connection, with its protocol and who it's from.
    /// `None` once the node has shut down. Waits for each handshake in turn,
    /// so a long-lived listener should use [`Node::incoming`] instead.
    pub async fn accept(&self) -> Option<Result<(Vec<u8>, Connection), String>> {
        Some(handshake(self.incoming().await?).await)
    }

    /// The next incoming connection attempt, before its handshake. Finish it
    /// with [`handshake`] on a task of its own: an attempt whose other end
    /// went away (a duplicate that arrived through a relay, say) takes a
    /// while to fail and would hold up every attempt behind it.
    pub async fn incoming(&self) -> Option<iroh::endpoint::Incoming> {
        self.endpoint.accept().await
    }
}

/// Completes an incoming connection: its protocol and the connection.
pub async fn handshake(incoming: iroh::endpoint::Incoming) -> Result<(Vec<u8>, Connection), String> {
    match tokio::time::timeout(Duration::from_secs(15), incoming).await {
        Ok(Ok(conn)) => Ok((conn.alpn().to_vec(), conn)),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("The handshake took too long.".into()),
    }
}

impl Node {

    pub async fn dial(&self, id: &str, alpn: &[u8]) -> Result<Connection, String> {
        self.dial_addr(EndpointAddr::from(parse_id(id)?), alpn).await
    }

    /// Dials a trusted device: where it was last reached first, which is
    /// quick, then wherever a fresh lookup says.
    pub async fn dial_peer(&self, peer: &Peer, alpn: &[u8]) -> Result<Connection, String> {
        if let Some(addr) = peer.addr.clone().filter(|a| !a.is_empty()) {
            if let Ok(Ok(conn)) = tokio::time::timeout(Duration::from_secs(8), self.endpoint.connect(addr, alpn)).await {
                return Ok(conn);
            }
        }
        self.dial(&peer.id, alpn).await
    }

    /// The addresses this node has learned for another, from talking to it.
    pub async fn known_addr(&self, id: &str) -> Option<EndpointAddr> {
        let id = parse_id(id).ok()?;
        let info = self.endpoint.remote_info(id).await?;
        let addr = EndpointAddr::from_parts(id, info.into_addrs().map(|a| a.into_addr()));
        (!addr.is_empty()).then_some(addr)
    }

    /// Dials with addresses already known, such as in tests.
    pub async fn dial_addr(&self, addr: EndpointAddr, alpn: &[u8]) -> Result<Connection, String> {
        let connect = self.endpoint.connect(addr, alpn);
        match tokio::time::timeout(Duration::from_secs(30), connect).await {
            Ok(Ok(conn)) => Ok(conn),
            Ok(Err(e)) => Err(format!("Couldn't reach the other device: {e}")),
            Err(_) => Err("The other device didn't answer.".to_string()),
        }
    }

    pub async fn close(&self) {
        self.endpoint.close().await;
    }
}

/// "direct" when the bytes go straight between the two devices, "relay"
/// when they go through a relay server.
pub fn route(conn: &Connection) -> &'static str {
    let paths = conn.paths();
    let selected = (&paths).into_iter().find(|p| p.is_selected());
    match selected {
        Some(p) if p.is_relay() => "relay",
        Some(_) => "direct",
        None => "connecting",
    }
}

/// The remote side's address as text.
pub fn remote(conn: &Connection) -> String {
    id_string(&conn.remote_id())
}

/// Reads framed messages from a stream.
pub struct Reader {
    recv: RecvStream,
    codec: Codec,
    buffer: Vec<u8>,
}

impl Reader {
    pub fn new(recv: RecvStream) -> Self {
        Self { recv, codec: Codec::new(), buffer: vec![0u8; 16 * 1024] }
    }

    /// The next message, `Ok(None)` when the stream ended cleanly.
    pub async fn next(&mut self) -> Result<Option<Message>, String> {
        loop {
            match self.codec.next() {
                Ok(Some(message)) => return Ok(Some(message)),
                Ok(None) => {}
                // A newer app may say things this build doesn't know.
                Err(DecodeError::Unknown(_)) => continue,
                Err(e @ DecodeError::TooLarge(_)) => return Err(e.to_string()),
            }
            match self.recv.read(&mut self.buffer).await.map_err(|e| e.to_string())? {
                None => return Ok(None),
                Some(n) => self.codec.feed(&self.buffer[..n]),
            }
        }
    }
}

pub async fn write(send: &mut SendStream, message: &Message) -> Result<(), String> {
    send.write_all(&encode(message)).await.map_err(|e| e.to_string())
}

/// Opens the message stream on a connection this side dialed. The first
/// message goes out at once: the other side only learns of a stream once
/// something is written on it.
pub async fn open_messages(conn: &Connection, first: &Message) -> Result<(SendStream, Reader), String> {
    let (mut send, recv) = conn.open_bi().await.map_err(|e| e.to_string())?;
    write(&mut send, first).await?;
    Ok((send, Reader::new(recv)))
}

pub async fn accept_messages(conn: &Connection) -> Result<(SendStream, Reader), String> {
    let (send, recv) = conn.accept_bi().await.map_err(|e| e.to_string())?;
    Ok((send, Reader::new(recv)))
}

/// Reads the one framed message that opens a unidirectional stream.
pub async fn read_header(recv: &mut RecvStream) -> Result<Message, String> {
    let length = recv.read_u32().await.map_err(|e| e.to_string())? as usize;
    if length > MAX_FRAME.min(64 * 1024) {
        return Err("The other side sent something unreadable.".into());
    }
    let mut body = vec![0u8; length];
    recv.read_exact(&mut body).await.map_err(|e| e.to_string())?;
    serde_json::from_slice(&body).map_err(|e| e.to_string())
}

/// Bytes between progress reports.
pub const PROGRESS_EVERY: u64 = 512 * 1024;

/// Sends one file on a stream of its own. Returns the bytes sent once the
/// other side has all of them.
pub async fn send_file<R, F>(conn: &Connection, id: u64, name: &str, size: i64, mut source: R, mut progress: F) -> Result<u64, String>
where
    R: AsyncRead + Unpin,
    F: FnMut(u64),
{
    let cut = |e: std::io::Error| format!("The connection was lost: {e}");
    let mut send = conn.open_uni().await.map_err(|e| format!("The connection was lost: {e}"))?;
    write(&mut send, &Message::FileStart { id, name: name.to_string(), size }).await?;
    let mut chunk = vec![0u8; 64 * 1024];
    let (mut done, mut told) = (0u64, 0u64);
    loop {
        let n = source.read(&mut chunk).await.map_err(|e| format!("Couldn't read the file: {e}"))?;
        if n == 0 {
            break;
        }
        send.write_all(&chunk[..n]).await.map_err(|e| cut(std::io::Error::other(e)))?;
        done += n as u64;
        if done - told >= PROGRESS_EVERY {
            told = done;
            progress(done);
        }
    }
    send.finish().map_err(|e| format!("The connection was lost: {e}"))?;
    // Returns once the other side has read everything, or gave up.
    let _ = send.stopped().await;
    Ok(done)
}

/// Receives the rest of a file stream whose `FileStart` was already read.
pub async fn receive_file<W, F>(recv: &mut RecvStream, mut dest: W, size: i64, mut progress: F) -> Result<u64, String>
where
    W: AsyncWrite + Unpin,
    F: FnMut(u64),
{
    let mut chunk = vec![0u8; 64 * 1024];
    let (mut done, mut told) = (0u64, 0u64);
    loop {
        match recv.read(&mut chunk).await.map_err(|e| format!("The connection was lost: {e}"))? {
            None => break,
            Some(n) => {
                dest.write_all(&chunk[..n]).await.map_err(|e| format!("Couldn't write the file: {e}"))?;
                done += n as u64;
                if done - told >= PROGRESS_EVERY {
                    told = done;
                    progress(done);
                }
            }
        }
    }
    dest.flush().await.map_err(|e| format!("Couldn't write the file: {e}"))?;
    if size >= 0 && done != size as u64 {
        return Err("The file didn't arrive whole.".into());
    }
    Ok(done)
}

/// Ends a conversation without losing its last words: finishes the stream,
/// then waits (briefly) for the other side to close the connection.
pub async fn finish(conn: &Connection, mut send: SendStream) {
    let _ = send.finish();
    let _ = tokio::time::timeout(Duration::from_secs(5), conn.closed()).await;
}

pub fn is_pair(alpn: &[u8]) -> bool {
    alpn == PAIR_ALPN
}

pub fn is_link(alpn: &[u8]) -> bool {
    alpn == LINK_ALPN
}
