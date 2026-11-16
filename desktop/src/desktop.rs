//! The desktop side of things: notifications, the clipboard, the theme.

use std::collections::HashMap;
use std::process::Stdio;
use std::time::Duration;

use futures_util::StreamExt;
use serde_json::{json, Value as Json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;
use zbus::zvariant::Value;
use zbus::Connection;

use crate::core::Event;

const NOTIFICATIONS: &str = "org.freedesktop.Notifications";
const NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";

pub struct Toast<'a> {
    pub replaces: u32,
    pub app: &'a str,
    pub icon: &'a str,
    pub title: &'a str,
    pub body: &'a str,
    /// Pairs of (key, label).
    pub actions: Vec<(String, String)>,
    pub urgent: bool,
}

/// Shows a desktop notification and returns the id the server gave it.
pub async fn notify(conn: &Connection, toast: Toast<'_>) -> Option<u32> {
    let actions: Vec<&str> = toast.actions.iter().flat_map(|(k, l)| [k.as_str(), l.as_str()]).collect();
    let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
    hints.insert("desktop-entry", Value::from("omalink"));
    hints.insert("urgency", Value::from(if toast.urgent { 2u8 } else { 1u8 }));
    // An incoming call stays up until it is dealt with.
    let timeout: i32 = if toast.urgent { 0 } else { -1 };
    let reply = conn
        .call_method(
            Some(NOTIFICATIONS),
            NOTIFICATIONS_PATH,
            Some(NOTIFICATIONS),
            "Notify",
            &(toast.app, toast.replaces, toast.icon, toast.title, toast.body, actions, hints, timeout),
        )
        .await
        .ok()?;
    reply.body().deserialize::<u32>().ok()
}

pub async fn close_notification(conn: &Connection, id: u32) {
    let _ = conn
        .call_method(Some(NOTIFICATIONS), NOTIFICATIONS_PATH, Some(NOTIFICATIONS), "CloseNotification", &(id,))
        .await;
}

/// Reports buttons pressed on, and dismissals of, our desktop notifications.
pub async fn watch_notifications(conn: Connection, tx: UnboundedSender<Event>) {
    let Ok(rule) = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(NOTIFICATIONS)
        .map(|b| b.build())
    else {
        return;
    };
    let Ok(mut signals) = zbus::MessageStream::for_match_rule(rule, &conn, Some(64)).await else { return };
    while let Some(Ok(message)) = signals.next().await {
        let header = message.header();
        let Some(member) = header.member() else { continue };
        let event = match member.as_str() {
            "ActionInvoked" => match message.body().deserialize::<(u32, String)>() {
                Ok((id, action)) => Event::ToastAction { id, action },
                Err(_) => continue,
            },
            "NotificationClosed" => match message.body().deserialize::<(u32, u32)>() {
                // Reason 2 is the user dismissing it; expiry and our own
                // CloseNotification calls must not dismiss it on the phone.
                Ok((id, reason)) => Event::ToastClosed { id, by_user: reason == 2 },
                Err(_) => continue,
            },
            _ => continue,
        };
        if tx.send(event).is_err() {
            return;
        }
    }
}

/// Reports text the user copies. `wl-paste --watch` runs a command on every
/// clipboard change; we have it print a line and read the clipboard ourselves,
/// so the content never has to be parsed out of a stream.
pub async fn watch_clipboard(tx: UnboundedSender<Event>) {
    loop {
        let child = Command::new("wl-paste")
            .args(["--type", "text", "--watch", "echo"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn();
        let Ok(mut child) = child else { return };
        let Some(stdout) = child.stdout.take() else { return };
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(_)) = lines.next_line().await {
            if let Some(text) = read_clipboard().await {
                if tx.send(Event::Copied(text)).is_err() {
                    return;
                }
            }
        }
        // The compositor restarted or wl-paste died; try again shortly.
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn read_clipboard() -> Option<String> {
    // Password managers flag what they copy; that never leaves this machine.
    let types = Command::new("wl-paste").arg("--list-types").output().await.ok()?;
    let types = String::from_utf8_lossy(&types.stdout);
    if types.contains("x-kde-passwordManagerHint") || types.contains("application/x-secret") {
        return None;
    }
    let output = Command::new("wl-paste").args(["--no-newline", "--type", "text"]).output().await.ok()?;
    if !output.status.success() || output.stdout.is_empty() || output.stdout.len() > 256 * 1024 {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

pub async fn write_clipboard(text: &str) -> bool {
    let child = Command::new("wl-copy").stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
    let Ok(mut child) = child else { return false };
    if let Some(mut stdin) = child.stdin.take() {
        if stdin.write_all(text.as_bytes()).await.is_err() {
            return false;
        }
    }
    child.wait().await.map(|s| s.success()).unwrap_or(false)
}

pub async fn set_mic_muted(muted: bool) {
    let _ = Command::new("wpctl")
        .args(["set-mute", "@DEFAULT_AUDIO_SOURCE@", if muted { "1" } else { "0" }])
        .status()
        .await;
}

fn parse_colors(toml: &str) -> serde_json::Map<String, Json> {
    let mut colors = serde_json::Map::new();
    for line in toml.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
        if value.starts_with('#') || key.trim() == "mode" {
            colors.insert(key.trim().to_string(), json!(value));
        }
    }
    colors
}

/// Follows the Omarchy theme and Hyprland's window rounding, live.
pub async fn watch_theme(tx: UnboundedSender<Event>) {
    let home = std::env::var("HOME").unwrap_or_default();
    let path = format!("{home}/.local/state/omarchy/current/theme/colors.toml");
    let on_hyprland = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some();
    let mut last = Json::Null;
    let mut radius = 0;
    let mut round = 0u32;
    loop {
        // Rounding changes rarely and asking costs a process, so ask less often.
        if on_hyprland && round % 5 == 0 {
            if let Ok(output) = Command::new("hyprctl").args(["-j", "getoption", "decoration:rounding"]).output().await {
                if let Ok(value) = serde_json::from_slice::<Json>(&output.stdout) {
                    radius = value.get("int").and_then(Json::as_i64).unwrap_or(radius);
                }
            }
        }
        round += 1;
        let mut theme = parse_colors(&tokio::fs::read_to_string(&path).await.unwrap_or_default());
        theme.insert("radius".into(), json!(radius));
        let theme = Json::Object(theme);
        if theme != last {
            last = theme.clone();
            if tx.send(Event::Theme(theme)).is_err() {
                return;
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_colors_and_ignores_the_rest() {
        let colors = parse_colors("mode = \"dark\"\n\naccent = \"#81a1c1\"\n# note\nname = 'x'\nred='#bf616a'\n");
        assert_eq!(colors.get("accent"), Some(&json!("#81a1c1")));
        assert_eq!(colors.get("red"), Some(&json!("#bf616a")));
        assert_eq!(colors.get("mode"), Some(&json!("dark")));
        assert!(!colors.contains_key("name"));
    }
}

/// Files to send from what was copied: copied files as they are, or a copied
/// image saved to a file first.
pub async fn clipboard_files() -> Result<Vec<std::path::PathBuf>, String> {
    let nothing = || "Copy a file or an image first, then press Ctrl+V here to send it.".to_string();
    let types = Command::new("wl-paste").arg("--list-types").output().await.map_err(|_| nothing())?;
    let types = String::from_utf8_lossy(&types.stdout).to_string();
    if types.lines().any(|t| t == "text/uri-list") {
        let list = Command::new("wl-paste").args(["--no-newline", "--type", "text/uri-list"]).output().await.map_err(|_| nothing())?;
        let paths: Vec<std::path::PathBuf> = String::from_utf8_lossy(&list.stdout)
            .lines()
            .filter_map(|line| line.trim().strip_prefix("file://").map(unescape))
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_file())
            .collect();
        if !paths.is_empty() {
            return Ok(paths);
        }
    }
    let image = ["image/png", "image/jpeg", "image/webp", "image/gif"].into_iter().find(|m| types.lines().any(|t| t == *m));
    let Some(mime) = image else { return Err(nothing()) };
    let bytes = Command::new("wl-paste").args(["--no-newline", "--type", mime]).output().await.map_err(|_| nothing())?.stdout;
    if bytes.is_empty() || bytes.len() > 100 * 1024 * 1024 {
        return Err(nothing());
    }
    let base = std::env::var("XDG_CACHE_HOME").unwrap_or_else(|_| format!("{}/.cache", std::env::var("HOME").unwrap_or_default()));
    let dir = std::path::PathBuf::from(base).join("omalink/outbox");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let extension = mime.trim_start_matches("image/").replace("jpeg", "jpg");
    let path = dir.join(format!("Pasted image {}.{extension}", local_stamp()));
    std::fs::write(&path, bytes).map_err(|e| format!("Couldn't save the copied image: {e}"))?;
    Ok(vec![path])
}

/// `%20` and friends back into the characters they stand for.
fn unescape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
            if let Some(byte) = hex {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// "2026-11-17 00.42.10", in local time, for file names.
fn local_stamp() -> String {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&now, &mut tm) };
    format!("{:04}-{:02}-{:02} {:02}.{:02}.{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec)
}

#[cfg(test)]
mod url_tests {
    #[test]
    fn unescapes_file_urls() {
        assert_eq!(super::unescape("/home/a/My%20File%E2%9C%93.txt"), "/home/a/My File✓.txt");
        assert_eq!(super::unescape("/plain/path"), "/plain/path");
        assert_eq!(super::unescape("/bad%zz%2"), "/bad%zz%2");
        assert_eq!(super::unescape("/é%é"), "/é%é");
    }
}
