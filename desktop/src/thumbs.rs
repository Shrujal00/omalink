//! Small pictures of files for the Files page.
//!
//! `ffmpegthumbnailer`, which Omarchy installs, makes them for images and
//! videos alike. Each is made once, in the background, into the cache, named
//! after the file's path, size and modification time so an edited file gets
//! a fresh one.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Semaphore;

use crate::core::Event;

/// What a file is, from its name: picks the glyph, and whether to make a
/// thumbnail at all.
pub fn kind(name: &str) -> &'static str {
    let extension = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match extension.as_str() {
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "heic" | "heif" | "avif" | "bmp" | "tif" | "tiff" => "image",
        "mp4" | "mkv" | "mov" | "webm" | "avi" | "3gp" | "m4v" => "video",
        "mp3" | "m4a" | "aac" | "ogg" | "opus" | "flac" | "wav" | "amr" => "audio",
        "zip" | "tar" | "gz" | "xz" | "zst" | "7z" | "rar" | "bz2" => "archive",
        "apk" | "aab" => "apk",
        "pdf" | "txt" | "md" | "doc" | "docx" | "odt" | "rtf" | "xls" | "xlsx" | "ods" | "csv" | "ppt" | "pptx" | "odp" | "epub" => "doc",
        _ => "other",
    }
}

fn dir() -> PathBuf {
    let base = std::env::var("XDG_CACHE_HOME").unwrap_or_else(|_| format!("{}/.cache", std::env::var("HOME").unwrap_or_default()));
    PathBuf::from(base).join("omalink/thumbs")
}

fn cached(source: &Path) -> Option<PathBuf> {
    let meta = std::fs::metadata(source).ok()?;
    let mut hash = DefaultHasher::new();
    source.hash(&mut hash);
    meta.len().hash(&mut hash);
    meta.modified().ok().hash(&mut hash);
    Some(dir().join(format!("{:016x}.jpg", hash.finish())))
}

/// At most two at a time, so a folder of videos doesn't swamp the machine.
fn permits() -> Arc<Semaphore> {
    static PERMITS: OnceLock<Arc<Semaphore>> = OnceLock::new();
    PERMITS.get_or_init(|| Arc::new(Semaphore::new(2))).clone()
}

/// Makes a thumbnail for the transfer `key` if its file is a picture or a
/// video, and reports it as `Event::Thumb`.
pub fn make(key: String, source: PathBuf, tx: UnboundedSender<Event>) {
    let name = source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if !matches!(kind(&name), "image" | "video") {
        return;
    }
    tokio::spawn(async move {
        let Some(out) = cached(&source) else { return };
        if !out.exists() {
            let _permit = permits().acquire_owned().await;
            let _ = std::fs::create_dir_all(dir());
            let temp = out.with_extension("tmp.jpg");
            let made = Command::new("ffmpegthumbnailer")
                .arg("-i")
                .arg(&source)
                .arg("-o")
                .arg(&temp)
                .args(["-s", "320", "-c", "jpeg", "-q", "7"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await
                .is_ok_and(|s| s.success());
            if !made || std::fs::rename(&temp, &out).is_err() {
                let _ = std::fs::remove_file(&temp);
                return;
            }
        }
        let _ = tx.send(Event::Thumb { key, path: out.to_string_lossy().into_owned() });
    });
}
