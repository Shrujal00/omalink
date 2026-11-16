//! The coordinator. Everything that happens — a click in the window, a frame
//! from the phone, a BlueZ signal — arrives here as an [`Event`] on one
//! channel and is handled on one task, so no state needs a lock.
//!
//! The window is told about state by topic: `emit("devices", json)`. It keeps
//! no truth of its own.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use omalink_proto::{
    number_key, permission, CallEntry, CallPhase, Contact, Conversation, InputKind, Message, Notification, PhoneMedia, PhoneStatus,
    PROTOCOL_VERSION, SERVICE_UUID,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
use zbus::Connection;

use omalink_net::{Conn, Node, Peer, Trust};
use omalink_proto::pair::{self, Shower};

use crate::desktop::{self, Toast};
use crate::link::Link;
use crate::net::Session;
use crate::{bluez, link, net, telephony, transfer};

pub type Emit = Arc<dyn Fn(&str, String) + Send + Sync>;

pub enum Event {
    Ui(String, Json),
    Bluez(bluez::Snapshot),
    PairRequest { device: String, code: String, confirm: bool, reply: Option<oneshot::Sender<bool>> },
    PairCancelled,
    PairDone { device: String, result: Result<(), String> },
    LinkUp(Link),
    LinkDown { serial: u64, reason: String },
    ConnectFailed(String),
    Peer { serial: u64, message: Message },
    Telephony(telephony::State),
    Copied(String),
    ToastAction { id: u32, action: String },
    ToastClosed { id: u32, by_user: bool },
    Theme(Json),
    TransferProgress { key: String, done: u64 },
    TransferEnd { key: String, done: u64, error: Option<String> },
    Thumb { key: String, path: String },
    Tick,

    // The network link.
    NetReady(Arc<Node>),
    NetFailed(String),
    NetIncoming { alpn: Vec<u8>, conn: Conn },
    NetDown { serial: u64, reason: String },
    NetPeer { serial: u64, message: Message },
    NetRoute { serial: u64, route: String },
    NetFileIncoming(Transfer),

    // Mirroring the phone's screen.
    MirrorLive { serial: u64, width: u32, height: u32 },
    MirrorFps { serial: u64, fps: u32 },
    MirrorKeyframe { serial: u64 },
    MirrorStreamEnd { serial: u64 },
    MirrorFailed { serial: u64, reason: String },
}

/// A screen-sharing session, as the Mirror page shows it.
#[derive(Default)]
struct Mirror {
    /// "idle"; "requested" until the phone starts the icon check; "verify"
    /// while the icon is on screen; "approving" while Android asks on the
    /// phone; "live"; or "ended", with `reason`.
    state: &'static str,
    icon: Option<usize>,
    width: u32,
    height: u32,
    control: bool,
    fps: u32,
    reason: String,
}

/// The desktop's side of pairing a phone over the network.
struct Pairing {
    until: Instant,
    attempts: u32,
    session: Option<Session>,
    phone_name: String,
    icon: Option<usize>,
}

/// How long pairing stays open, and how many wrong taps it forgives.
const PAIR_WINDOW: Duration = Duration::from_secs(180);
const PAIR_ATTEMPTS: u32 = 3;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    /// Bluetooth address of the phone this desktop is linked to.
    pub device: String,
    pub clipboard: bool,
    pub toasts: bool,
    pub calls: bool,
    /// The relay to fall back on, or empty for the public ones.
    pub relay: String,
    /// Thread → time of its newest message when it was last opened here.
    /// The phone can't always be told a thread was read, so the desktop
    /// remembers for itself.
    pub seen: HashMap<i64, i64>,
}

impl Default for Config {
    fn default() -> Self {
        Self { device: String::new(), clipboard: true, toasts: true, calls: true, relay: String::new(), seen: HashMap::new() }
    }
}

impl Config {
    fn path() -> std::path::PathBuf {
        let base = std::env::var("XDG_CONFIG_HOME")
            .unwrap_or_else(|_| format!("{}/.config", std::env::var("HOME").unwrap_or_default()));
        std::path::PathBuf::from(base).join("omalink/config.json")
    }

    fn load() -> Self {
        std::fs::read(Self::path())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Written beside the target and renamed, so a crash can't leave half a file.
    fn save(&self) {
        let path = Self::path();
        let Some(dir) = path.parent() else { return };
        let _ = std::fs::create_dir_all(dir);
        let temp = path.with_extension("json.tmp");
        if std::fs::write(&temp, serde_json::to_vec_pretty(self).unwrap_or_default()).is_ok() {
            let _ = std::fs::rename(temp, path);
        }
    }
}

#[derive(Serialize, Clone)]
pub struct Clip {
    pub text: String,
    pub time: i64,
    pub from: &'static str,
}

/// What outlives a restart: finished transfers and the clipboard history,
/// kept readable by the owner only (copied text can be private).
#[derive(Serialize, Deserialize, Default)]
struct History {
    #[serde(default)]
    transfers: Vec<StoredTransfer>,
    #[serde(default)]
    clips: Vec<StoredClip>,
}

#[derive(Serialize, Deserialize)]
struct StoredTransfer {
    key: String,
    name: String,
    size: i64,
    done: u64,
    incoming: bool,
    state: String,
    #[serde(default)]
    error: String,
    path: String,
    time: i64,
    #[serde(default)]
    via: String,
    #[serde(default)]
    kind: String,
}

#[derive(Serialize, Deserialize)]
struct StoredClip {
    text: String,
    time: i64,
    from: String,
}

/// The one of `options` that `text` names, or the first.
fn known(text: &str, options: &[&'static str]) -> &'static str {
    options.iter().copied().find(|o| *o == text).unwrap_or(options[0])
}

impl History {
    fn path() -> std::path::PathBuf {
        let base = std::env::var("XDG_STATE_HOME")
            .unwrap_or_else(|_| format!("{}/.local/state", std::env::var("HOME").unwrap_or_default()));
        std::path::PathBuf::from(base).join("omalink/history.json")
    }

    fn load() -> (Vec<Transfer>, Vec<Clip>) {
        let history: History = std::fs::read(Self::path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let transfers = history
            .transfers
            .into_iter()
            .map(|t| Transfer {
                key: t.key,
                name: t.name,
                size: t.size,
                done: t.done,
                incoming: t.incoming,
                state: known(&t.state, &["done", "failed"]),
                error: t.error,
                path: t.path,
                time: t.time,
                via: known(&t.via, &["network", "bluetooth"]),
                kind: known(&t.kind, &["other", "image", "video", "audio", "archive", "apk", "doc"]),
                eta: -1,
                ..Default::default()
            })
            .collect();
        let clips = history
            .clips
            .into_iter()
            .map(|c| Clip { text: c.text, time: c.time, from: known(&c.from, &["desktop", "phone"]) })
            .collect();
        (transfers, clips)
    }

    fn save(transfers: &[Transfer], clips: &[Clip]) {
        let history = History {
            transfers: transfers
                .iter()
                .filter(|t| t.state == "done" || t.state == "failed")
                .map(|t| StoredTransfer {
                    key: t.key.clone(),
                    name: t.name.clone(),
                    size: t.size,
                    done: t.done,
                    incoming: t.incoming,
                    state: t.state.into(),
                    error: t.error.clone(),
                    path: t.path.clone(),
                    time: t.time,
                    via: t.via.into(),
                    kind: t.kind.into(),
                })
                .collect(),
            clips: clips.iter().map(|c| StoredClip { text: c.text.clone(), time: c.time, from: c.from.into() }).collect(),
        };
        let path = Self::path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let temp = path.with_extension("json.tmp");
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let written = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temp)
            .and_then(|mut f| f.write_all(&serde_json::to_vec(&history).unwrap_or_default()));
        if written.is_ok() {
            let _ = std::fs::rename(temp, path);
        }
    }
}

/// A file going to or coming from the phone, as the Files page lists it.
#[derive(Serialize, Clone, Default)]
pub struct Transfer {
    /// See `transfer::key`.
    pub key: String,
    pub name: String,
    pub size: i64,
    pub done: u64,
    pub incoming: bool,
    /// "waiting" for the phone to answer, "active", "done" or "failed".
    pub state: &'static str,
    pub error: String,
    /// Where the file is on this computer.
    pub path: String,
    pub time: i64,
    /// "bluetooth" when Bluetooth arranged it, "network" when the network
    /// link carries it.
    pub via: &'static str,
    /// See `thumbs::kind`.
    pub kind: &'static str,
    /// A file URL of its thumbnail, once there is one.
    pub thumb: String,
    /// Bytes a second, and seconds left (-1 when unknown), while it moves.
    pub speed: f64,
    pub eta: i64,
    /// The last progress report, for working out the speed.
    #[serde(skip)]
    pub sample: Option<(Instant, u64)>,
}

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

pub struct Core {
    pub emit: Emit,
    tx: UnboundedSender<Event>,
    system: Option<Connection>,
    session: Option<Connection>,
    pub demo: bool,
    started: Instant,

    config: Config,
    bluez: bluez::Snapshot,
    pending_pair: Option<oneshot::Sender<bool>>,

    link: Option<Link>,
    connecting: Option<Instant>,
    paused: bool,
    next_attempt: Instant,
    backoff: u64,
    link_error: String,
    peer_name: String,
    last_heard: Instant,
    ticks: u64,

    telephony: telephony::State,
    phone_call: (CallPhase, String, String),
    call_phase: &'static str,
    call_since: i64,
    call_toast: Option<u32>,
    muted: bool,

    pub status: Option<PhoneStatus>,
    pub granted: Vec<String>,
    pub missing: Vec<String>,
    pub notifications: Vec<Notification>,
    toasts: HashMap<String, u32>,
    pub conversations: Vec<Conversation>,
    open_thread: Option<i64>,
    pub contacts: Vec<Contact>,
    names: HashMap<String, String>,
    pub call_log: Vec<CallEntry>,
    pub clips: Vec<Clip>,
    last_clip: String,
    sms_id: u64,
    pub transfers: Vec<Transfer>,
    transfer_id: u64,
    published: String,

    node: Option<Arc<Node>>,
    net_state: &'static str,
    net_error: String,
    trust: Trust,
    net_link: Option<Session>,
    net_route: String,
    pairing: Option<Pairing>,
    /// An icon verification under way, on the session with this serial.
    verify: Option<(u64, Shower)>,
    mirror: Mirror,

    /// The phone's gallery as far as it has been loaded, newest first, with
    /// each thumbnail as a file URL.
    gallery: Vec<PhoneMedia>,
    gallery_cursor: i64,
    gallery_more: bool,
    gallery_loading: bool,
}

fn arg_str(args: &Json, key: &str) -> String {
    args.get(key).and_then(Json::as_str).unwrap_or_default().to_string()
}

fn arg_bool(args: &Json, key: &str) -> bool {
    args.get(key).and_then(Json::as_bool).unwrap_or(false)
}

impl Core {
    pub fn send_ui(&self, topic: &str, value: Json) {
        (self.emit)(topic, value.to_string());
    }

    fn note(&self, text: impl Into<String>) {
        let text = text.into();
        // Also on stderr, so a problem at startup is visible when the app
        // was started without a window.
        eprintln!("omalink: {text}");
        self.send_ui("note", json!({ "text": text }));
    }

    /// Bluetooth when it's up, the network otherwise.
    fn to_phone(&self, message: Message) -> bool {
        if let Some(link) = &self.link {
            link.send(message);
            true
        } else if let Some(session) = &self.net_link {
            session.send(message);
            true
        } else {
            self.note("The phone isn't connected.");
            false
        }
    }

    /// Whether the phone can be talked to at all.
    fn connected(&self) -> bool {
        self.link.is_some() || self.net_link.is_some()
    }

    fn device(&self) -> Option<&bluez::Device> {
        self.bluez.devices.iter().find(|d| d.address == self.config.device)
    }

    fn name_for(&self, number: &str) -> String {
        self.names.get(&number_key(number)).cloned().unwrap_or_default()
    }

    // What the window is told.

    fn emit_bluetooth(&self) {
        self.send_ui("adapter", json!(self.bluez.adapter));
        self.send_ui("devices", json!(self.bluez.devices));
    }

    fn emit_link(&self) {
        let state = if self.connected() {
            "connected"
        } else if self.config.device.is_empty() {
            "none"
        } else if self.connecting.is_some() {
            "connecting"
        } else if self.paused {
            "paused"
        } else {
            "waiting"
        };
        let name = match self.device() {
            Some(d) if !d.name.is_empty() => d.name.clone(),
            _ => self.peer_name.clone(),
        };
        let name = if name.is_empty() {
            self.net_link.as_ref().and_then(|s| self.trust.name(&s.id)).unwrap_or_default().to_string()
        } else {
            name
        };
        self.send_ui(
            "link",
            json!({
                "state": state,
                "via": if self.link.is_some() { "bluetooth" } else if self.net_link.is_some() { "network" } else { "" },
                "address": self.config.device,
                "name": name,
                "error": if self.connected() { "" } else { self.link_error.as_str() },
                "stale": self.link.is_none() && self.link_error == bluez::STALE_PAIRING,
                "calls": self.telephony.modem.is_some(),
            }),
        );
    }

    fn emit_net(&self) {
        let connected = self.net_link.as_ref().map(|s| s.id.clone()).unwrap_or_default();
        let phones: Vec<Json> = self
            .trust
            .peers
            .iter()
            .map(|p| json!({ "id": p.id, "name": p.name, "added": p.added, "connected": p.id == connected }))
            .collect();
        let pairing = self.pairing.as_ref().map(|p| {
            json!({
                "phone": p.phone_name,
                "icon": p.icon.map(|i| pair::ICONS[i].0).unwrap_or(""),
                "icon_name": p.icon.map(|i| pair::ICONS[i].1).unwrap_or(""),
                "attempts": p.attempts,
                "left": PAIR_ATTEMPTS.saturating_sub(p.attempts),
            })
        });
        self.send_ui(
            "net",
            json!({
                "state": self.net_state,
                "error": self.net_error,
                "id": self.node.as_ref().map(|n| n.id()).unwrap_or_default(),
                "name": hostname(),
                "connected": self.net_link.is_some(),
                "route": if self.net_link.is_some() { self.net_route.as_str() } else { "" },
                "relay": self.config.relay,
                "phones": phones,
                "pairing": pairing,
            }),
        );
    }

    fn open_pairing(&mut self) {
        let Some(node) = self.node.clone() else {
            return self.note("Networking hasn't started yet.");
        };
        node.set_pairing(Some(&hostname()));
        self.pairing = Some(Pairing { until: Instant::now() + PAIR_WINDOW, attempts: 0, session: None, phone_name: String::new(), icon: None });
        // A phone on Bluetooth learns where to find this computer even if
        // they're not on the same network.
        if let Some(link) = &self.link {
            link.send(Message::NetInvite { id: node.id(), name: hostname() });
        }
        self.emit_net();
    }

    fn close_pairing(&mut self) {
        if let Some(pairing) = self.pairing.take() {
            if let Some(session) = pairing.session {
                if self.verify.as_ref().is_some_and(|(serial, _)| *serial == session.serial) {
                    self.verify = None;
                }
                net::retire(session);
            }
        }
        if let Some(node) = &self.node {
            node.set_pairing(None);
        }
        self.emit_net();
    }

    /// The desktop's half of icon verification, on a pairing session or the
    /// link. Returns false for messages that aren't part of it.
    fn handle_verify(&mut self, serial: u64, message: &Message) -> bool {
        let on_pairing = self.pairing.as_ref().and_then(|p| p.session.as_ref()).is_some_and(|s| s.serial == serial);
        let session = if on_pairing {
            self.pairing.as_ref().and_then(|p| p.session.as_ref())
        } else {
            self.net_link.as_ref().filter(|s| s.serial == serial)
        };
        let Some(session) = session else { return false };
        let Some(node) = &self.node else { return false };
        match message {
            Message::VerifyCommit { purpose, commit } => {
                // A pairing session verifies pairing; the link only mirroring.
                let allowed = if on_pairing { purpose == pair::PAIR } else { purpose == pair::MIRROR && self.mirror.state == "requested" };
                let ours = omalink_net::id_bytes(&node.endpoint.id());
                let theirs = omalink_net::id_bytes(&session.conn.remote_id());
                let nonce = omalink_net::random32();
                match Shower::new(purpose, &ours, &theirs, commit, &nonce).ok().filter(|_| allowed) {
                    Some(shower) => {
                        session.send(Message::VerifyNonce { nonce: pair::to_hex(&nonce) });
                        self.verify = Some((serial, shower));
                    }
                    None => session.send(Message::VerifyDone { ok: false }),
                }
            }
            Message::VerifyReveal { nonce } => {
                let Some((_, shower)) = self.verify.as_mut().filter(|(s, _)| *s == serial) else { return true };
                match shower.reveal(nonce) {
                    Ok(sas) => {
                        // Development builds say which icon is on screen, so
                        // a scripted phone can tap it; release builds never do.
                        #[cfg(debug_assertions)]
                        eprintln!("omalink: showing icon {}", pair::ICONS[sas.icon].1);
                        if let Some(p) = self.pairing.as_mut().filter(|_| on_pairing) {
                            p.icon = Some(sas.icon);
                        }
                        if !on_pairing {
                            self.mirror.state = "verify";
                            self.mirror.icon = Some(sas.icon);
                        }
                    }
                    Err(_) => {
                        session.send(Message::VerifyDone { ok: false });
                        self.verify = None;
                        if !on_pairing {
                            self.end_mirror("The phone's answer didn't check out. Nothing was shared.");
                        }
                    }
                }
                if on_pairing {
                    self.emit_net();
                } else {
                    self.emit_mirror();
                }
            }
            Message::VerifyResult { ok } => {
                let shown = self.verify.take().filter(|(s, _)| *s == serial).and_then(|(_, shower)| shower.icon());
                let ok = *ok && shown.is_some();
                session.send(Message::VerifyDone { ok });
                if on_pairing {
                    self.finish_pairing(ok);
                } else if ok {
                    self.mirror.state = "approving";
                    self.mirror.icon = None;
                    self.emit_mirror();
                } else {
                    self.end_mirror("The icon tapped on the phone wasn't the one shown here. Nothing was shared.");
                }
            }
            _ => return false,
        }
        true
    }

    fn finish_pairing(&mut self, ok: bool) {
        let Some(pairing) = self.pairing.as_mut() else { return };
        if ok {
            let Some(session) = pairing.session.take() else { return };
            let name = if pairing.phone_name.is_empty() { "Phone".to_string() } else { pairing.phone_name.clone() };
            self.trust.add(Peer { id: session.id.clone(), name: name.clone(), added: now_ms(), addr: None });
            net::retire(session);
            self.close_pairing();
            self.note(format!("Paired with {name} over the network."));
        } else {
            pairing.attempts += 1;
            pairing.icon = None;
            if let Some(session) = pairing.session.take() {
                net::retire(session);
            }
            if pairing.attempts >= PAIR_ATTEMPTS {
                self.close_pairing();
                self.note("Pairing stopped after too many wrong icons. Start again if that was you.");
            } else {
                self.note("That wasn't the icon on this screen. Try again on the phone.");
                self.emit_net();
            }
        }
    }

    fn emit_mirror(&self) {
        let m = &self.mirror;
        self.send_ui(
            "mirror",
            json!({
                "state": if m.state.is_empty() { "idle" } else { m.state },
                "icon": m.icon.map(|i| pair::ICONS[i].0).unwrap_or(""),
                "icon_name": m.icon.map(|i| pair::ICONS[i].1).unwrap_or(""),
                "width": m.width,
                "height": m.height,
                "control": m.control,
                "fps": m.fps,
                "reason": m.reason,
                "available": self.net_link.is_some(),
                "route": self.net_route,
            }),
        );
    }

    fn end_mirror(&mut self, reason: &str) {
        let was = self.mirror.state;
        self.mirror = Mirror { state: if was.is_empty() || was == "idle" { "idle" } else { "ended" }, reason: reason.to_string(), ..Default::default() };
        if self.verify.as_ref().is_some_and(|(s, _)| self.net_link.as_ref().is_some_and(|l| l.serial == *s)) {
            self.verify = None;
        }
        crate::video::clear();
        self.emit_mirror();
    }

    /// Input for the phone during a live, verified session.
    fn to_mirror(&self, message: Message) {
        if self.mirror.state == "live" && self.mirror.control {
            if let Some(session) = &self.net_link {
                session.send(message);
            }
        }
    }

    fn emit_settings(&self) {
        self.send_ui("settings", json!(self.config));
    }

    /// Counts `reading` as read up to its newest message, then clears the
    /// unread count of every thread with nothing newer than was read here.
    /// `fresh` says the counts are straight from the phone, not yet cleared.
    fn apply_seen(&mut self, reading: Option<i64>, fresh: bool) {
        let before = self.config.seen.clone();
        if let Some(c) = self.conversations.iter().find(|c| Some(c.thread_id) == reading && c.unread > 0) {
            self.config.seen.insert(c.thread_id, c.time);
        }
        // Threads the phone now calls read, or no longer lists, need no entry.
        if fresh && !self.conversations.is_empty() {
            let unread: Vec<i64> = self.conversations.iter().filter(|c| c.unread > 0).map(|c| c.thread_id).collect();
            self.config.seen.retain(|thread, _| unread.contains(thread));
        }
        for c in &mut self.conversations {
            if self.config.seen.get(&c.thread_id).is_some_and(|&seen| seen >= c.time) {
                c.unread = 0;
            }
        }
        if self.config.seen != before {
            self.config.save();
        }
    }

    /// Thumbnails for whatever transfers are listed; the demo's start with files.
    pub fn make_thumbs(&self) {
        for t in &self.transfers {
            crate::thumbs::make(t.key.clone(), std::path::PathBuf::from(&t.path), self.tx.clone());
        }
    }

    pub fn emit_phone(&self) {
        self.send_ui("status", json!(self.status));
        self.send_ui("permissions", json!({ "granted": self.granted, "missing": self.missing }));
        self.emit_notifications();
        self.send_ui("conversations", json!(self.conversations));
        self.send_ui("call_log", json!(self.call_log));
        self.emit_contacts();
        self.emit_wallpaper();
        self.send_ui("transfers", json!(self.transfers));
        self.send_ui("clips", json!(self.clips));
    }

    fn cache_dir() -> std::path::PathBuf {
        let base = std::env::var("XDG_CACHE_HOME")
            .unwrap_or_else(|_| format!("{}/.cache", std::env::var("HOME").unwrap_or_default()));
        std::path::PathBuf::from(base).join("omalink")
    }

    fn icon_path(package: &str) -> std::path::PathBuf {
        let safe: String = package.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '_').collect();
        Self::cache_dir().join("icons").join(format!("{safe}.png"))
    }

    /// Pictures from the linked phone, kept apart from any other phone's.
    fn phone_cache(&self) -> std::path::PathBuf {
        let device: String = self.config.device.chars().filter(char::is_ascii_alphanumeric).collect();
        Self::cache_dir().join(if device.is_empty() { "phone".to_string() } else { device })
    }

    fn photo_path(&self, id: i64) -> std::path::PathBuf {
        self.phone_cache().join(format!("photos/{id}.jpg"))
    }

    /// The stored wallpaper, if there is one. Each new wallpaper gets a new
    /// name, because the window keeps showing a file it has already loaded.
    fn wallpaper_path(&self) -> Option<std::path::PathBuf> {
        std::fs::read_dir(self.phone_cache()).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("wallpaper") && n.ends_with(".jpg"))
        })
    }

    fn store_wallpaper(&self, image: &str) -> bool {
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(image.as_bytes()) else { return false };
        let old = self.wallpaper_path();
        if old.as_ref().and_then(|p| std::fs::read(p).ok()).is_some_and(|had| had == bytes) {
            return false;
        }
        let dir = self.phone_cache();
        let _ = std::fs::create_dir_all(&dir);
        if std::fs::write(dir.join(format!("wallpaper-{}.jpg", now_ms())), bytes).is_err() {
            return false;
        }
        if let Some(old) = old {
            let _ = std::fs::remove_file(old);
        }
        true
    }

    fn store_image(path: &std::path::Path, data: &str) -> bool {
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data.as_bytes()) else { return false };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        std::fs::write(path, bytes).is_ok()
    }

    /// Contacts as the window wants them: the picture as a file, or nothing.
    fn emit_contacts(&self) {
        let items: Vec<Json> = self
            .contacts
            .iter()
            .map(|c| {
                let path = self.photo_path(c.photo);
                let photo = if c.photo != 0 && path.exists() { format!("file://{}", path.display()) } else { String::new() };
                json!({ "name": c.name, "numbers": c.numbers, "photo": photo })
            })
            .collect();
        self.send_ui("contacts", json!(items));
    }

    /// Newest first, and only the last fifty.
    /// Keeps the Files and Clipboard history for next time.
    fn save_history(&self) {
        if !self.demo {
            History::save(&self.transfers, &self.clips);
        }
    }

    fn add_transfer(&mut self, transfer: Transfer) {
        self.transfers.insert(0, transfer);
        self.transfers.truncate(50);
        self.send_ui("transfers", json!(self.transfers));
    }

    fn route(addrs: Vec<String>, port: u16, key: &str) -> transfer::Route {
        let key = base64::engine::general_purpose::STANDARD.decode(key.as_bytes()).unwrap_or_default();
        transfer::Route { addrs, port, key }
    }

    fn emit_gallery(&self) {
        self.send_ui("gallery", json!({ "items": self.gallery, "more": self.gallery_more, "loading": self.gallery_loading }));
    }

    fn load_gallery(&mut self, more: bool) {
        if more && (!self.gallery_more || self.gallery_loading) {
            return;
        }
        let before = if more { self.gallery_cursor } else { 0 };
        if self.to_phone(Message::GetPhotos { limit: 60, before }) {
            self.gallery_loading = true;
            self.emit_gallery();
        }
    }

    fn emit_wallpaper(&self) {
        let path = self.wallpaper_path().filter(|_| !self.demo);
        self.send_ui("wallpaper", json!(path.map(|p| format!("file://{}", p.display())).unwrap_or_default()));
    }

    /// App icons arrive inline; the window and the notification daemon both
    /// want a file, so each package's icon is written once to the cache.
    fn store_icon(notification: &mut Notification) {
        if notification.icon.is_empty() {
            return;
        }
        let path = Self::icon_path(&notification.package);
        if !path.exists() {
            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(notification.icon.as_bytes()) {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = std::fs::write(&path, bytes);
            }
        }
        notification.icon.clear();
    }

    fn emit_notifications(&self) {
        let items: Vec<Json> = self
            .notifications
            .iter()
            .map(|n| {
                let mut value = json!(n);
                let path = Self::icon_path(&n.package);
                value["icon"] = json!(if path.exists() { format!("file://{}", path.display()) } else { String::new() });
                value
            })
            .collect();
        self.send_ui("notifications", json!(items));
    }

    /// One call at a time is shown. Hands-free knows most, so it wins; the
    /// phone app's own report covers the case where hands-free isn't up.
    fn call_view(&self) -> (&'static str, String, Option<String>) {
        let rank = |state: &str| match state {
            "incoming" | "waiting" => 0,
            "active" => 1,
            "dialing" | "alerting" => 2,
            _ => 3,
        };
        if let Some(call) = self.telephony.calls.iter().min_by_key(|c| rank(&c.state)) {
            let phase = match call.state.as_str() {
                "incoming" | "waiting" => "incoming",
                "dialing" | "alerting" => "dialing",
                _ => "active",
            };
            let number = if call.number.is_empty() { self.phone_call.1.clone() } else { call.number.clone() };
            return (phase, number, Some(call.path.clone()));
        }
        match self.phone_call.0 {
            CallPhase::Ringing => ("incoming", self.phone_call.1.clone(), None),
            CallPhase::Active => ("active", self.phone_call.1.clone(), None),
            CallPhase::Idle => ("idle", String::new(), None),
        }
    }

    async fn emit_call(&mut self) {
        let (phase, number, path) = self.call_view();
        let mut name = self.name_for(&number);
        if name.is_empty() && number == self.phone_call.1 {
            name = self.phone_call.2.clone();
        }
        if phase != self.call_phase {
            if phase == "active" {
                self.call_since = now_ms();
            }
            if phase == "idle" && self.muted {
                // Never leave the microphone muted after the call that muted it.
                self.muted = false;
                desktop::set_mic_muted(false).await;
            }
            if let (Some(id), Some(session)) = (self.call_toast.take(), &self.session) {
                desktop::close_notification(session, id).await;
            }
            if phase == "incoming" {
                if let Some(session) = &self.session {
                    let who = if !name.is_empty() { name.clone() } else if !number.is_empty() { number.clone() } else { "Unknown caller".into() };
                    self.call_toast = desktop::notify(
                        session,
                        Toast {
                            replaces: 0,
                            app: "Omalink",
                            icon: "call-start",
                            title: "Incoming call",
                            body: &who,
                            actions: vec![("answer".into(), "Answer".into()), ("decline".into(), "Decline".into())],
                            urgent: true,
                        },
                    )
                    .await;
                }
            }
            self.call_phase = phase;
        }
        self.send_ui(
            "call",
            json!({
                "phase": phase,
                "number": number,
                "name": name,
                "since": self.call_since,
                "audio": path.is_some(),
                "muted": self.muted,
            }),
        );
    }

    fn status_path() -> std::path::PathBuf {
        let base = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
        std::path::PathBuf::from(base).join("omalink/status.json")
    }

    /// A one-line summary for things outside the app, such as the bar icon.
    /// Written only when it changes, beside the target and renamed into place.
    fn publish(&mut self) {
        if self.demo {
            return;
        }
        let (phase, _, _) = self.call_view();
        let summary = json!({
            "running": true,
            "state": if self.connected() { "connected" } else if self.config.device.is_empty() && self.trust.peers.is_empty() { "none" } else { "waiting" },
            "name": self.device().map(|d| d.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| self.peer_name.clone()),
            "battery": self.status.as_ref().filter(|_| self.connected()).map(|s| s.battery).unwrap_or(-1),
            "charging": self.status.as_ref().is_some_and(|s| s.charging),
            "unread": self.conversations.iter().map(|c| c.unread).sum::<u32>(),
            "notifications": self.notifications.len(),
            "call": phase,
        })
        .to_string();
        if summary == self.published {
            return;
        }
        let path = Self::status_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let temp = path.with_extension("json.tmp");
        if std::fs::write(&temp, &summary).is_ok() && std::fs::rename(&temp, &path).is_ok() {
            self.published = summary;
        }
    }

    // Connecting.

    fn try_connect(&mut self) {
        if self.demo || self.link.is_some() || self.connecting.is_some() || self.paused {
            return;
        }
        let Some(system) = self.system.clone() else { return };
        let Some(device) = self.device() else { return };
        if !device.paired || !self.bluez.adapter.as_ref().is_some_and(|a| a.powered) {
            return;
        }
        let path = device.path.clone();
        let tx = self.tx.clone();
        self.connecting = Some(Instant::now());
        self.emit_link();
        tokio::spawn(async move {
            if let Err(e) = bluez::connect_profile(&system, &path, SERVICE_UUID).await {
                let _ = tx.send(Event::ConnectFailed(e));
            }
        });
    }

    fn connect_calls(&self) {
        if !self.config.calls || self.telephony.modem.is_some() {
            return;
        }
        let (Some(system), Some(device)) = (self.system.clone(), self.device()) else { return };
        if !device.calls {
            return;
        }
        let path = device.path.clone();
        tokio::spawn(async move {
            // Only hands-free: a plain Connect would also route the phone's
            // music to this computer, which nobody asked for.
            let _ = bluez::connect_profile(&system, &path, bluez::HFP_AG_UUID).await;
        });
    }

    fn drop_link(&mut self, reason: &str) {
        if self.link.take().is_some() {
            self.link_error = reason.to_string();
            self.phone_call = (CallPhase::Idle, String::new(), String::new());
            // Whatever the phone hadn't answered yet, it won't now. Files on
            // the network carry on without Bluetooth.
            for t in self.transfers.iter_mut().filter(|t| t.state == "waiting" && t.via != "network") {
                t.state = "failed";
                t.error = "The phone disconnected.".into();
            }
            self.send_ui("transfers", json!(self.transfers));
            self.save_history();
        }
        self.connecting = None;
        self.emit_link();
    }

    fn retry_later(&mut self) {
        self.backoff = (self.backoff * 2).clamp(4, 60);
        self.next_attempt = Instant::now() + Duration::from_secs(self.backoff);
    }

    fn retry_now(&mut self) {
        self.paused = false;
        self.backoff = 0;
        self.next_attempt = Instant::now();
        self.try_connect();
    }

    // Events.

    pub async fn handle(&mut self, event: Event) {
        match event {
            Event::Ui(name, args) => self.handle_ui(&name, &args).await,
            Event::Theme(theme) => self.send_ui("theme", theme),

            Event::Bluez(snapshot) => {
                let had_device = self.device().is_some_and(|d| d.paired);
                self.bluez = snapshot;
                self.emit_bluetooth();
                self.emit_link();
                // The phone was just paired, or Bluetooth just came on.
                if !had_device && self.device().is_some_and(|d| d.paired) {
                    self.retry_now();
                }
            }
            Event::PairRequest { device, code, confirm, reply } => {
                self.pending_pair = reply;
                let name = self.bluez.devices.iter().find(|d| d.path == device).map(|d| d.name.clone()).unwrap_or_default();
                self.send_ui("pairing", json!({ "device": device, "name": name, "code": code, "confirm": confirm, "busy": true }));
            }
            Event::PairCancelled => {
                self.pending_pair = None;
                self.send_ui("pairing", Json::Null);
            }
            Event::PairDone { device, result } => {
                self.pending_pair = None;
                self.send_ui("pairing", Json::Null);
                match result {
                    Ok(()) => {
                        if let Some(d) = self.bluez.devices.iter().find(|d| d.path == device) {
                            self.config.device = d.address.clone();
                            self.config.save();
                        }
                        if let (Some(system), Some(adapter)) = (&self.system, &self.bluez.adapter) {
                            let _ = bluez::set_discovering(system, &adapter.path, false).await;
                        }
                        self.emit_link();
                        self.retry_now();
                    }
                    Err(e) => self.note(e),
                }
            }

            Event::LinkUp(link) => {
                // BlueZ only opens this channel when we ask, but be strict:
                // nothing but the chosen phone gets to speak the protocol.
                if self.device().is_none_or(|d| d.path != link.device) {
                    return;
                }
                link.send(Message::Hello { version: PROTOCOL_VERSION, name: hostname() });
                link.send(Message::Sync {
                    what: ["status", "permissions", "notifications", "conversations", "call_log", "contacts", "wallpaper"]
                        .map(String::from)
                        .to_vec(),
                });
                if let (Some(node), true) = (&self.node, self.pairing.is_some()) {
                    link.send(Message::NetInvite { id: node.id(), name: hostname() });
                }
                self.link = Some(link);
                self.connecting = None;
                self.backoff = 0;
                self.link_error.clear();
                self.last_heard = Instant::now();
                self.emit_link();
                self.connect_calls();
            }
            Event::LinkDown { serial, reason } => {
                if self.link.as_ref().is_some_and(|l| l.serial == serial) {
                    self.drop_link(&reason);
                    self.retry_later();
                    self.emit_call().await;
                }
            }
            Event::ConnectFailed(reason) => {
                self.connecting = None;
                // A thrown-away pairing won't come back: stop hammering the phone.
                if reason == bluez::STALE_PAIRING {
                    self.backoff = 60;
                }
                self.link_error = reason;
                self.retry_later();
                self.emit_link();
            }
            Event::Peer { serial, message } => {
                if self.link.as_ref().is_some_and(|l| l.serial == serial) {
                    self.last_heard = Instant::now();
                    self.handle_peer(message).await;
                }
            }

            Event::Telephony(state) => {
                self.telephony = state;
                self.emit_link();
                self.emit_call().await;
            }

            Event::Copied(text) => {
                // wl-paste reports what is already on the clipboard when it
                // starts; that wasn't copied just now. And what we put there
                // ourselves comes straight back as a change.
                if self.started.elapsed() < Duration::from_secs(2) || text == self.last_clip || text.trim().is_empty() {
                    self.last_clip = text;
                    return;
                }
                self.last_clip = text.clone();
                if self.config.clipboard && self.connected() {
                    self.push_clip(&text, "desktop");
                    self.to_phone(Message::Clipboard { text });
                }
            }

            Event::ToastAction { id, action } => {
                if self.call_toast == Some(id) {
                    let name = if action == "answer" { "answer" } else { "hangup" };
                    Box::pin(self.handle_ui(name, &Json::Null)).await;
                } else if let Some(key) = self.toasts.iter().find(|(_, v)| **v == id).map(|(k, _)| k.clone()) {
                    match action.strip_prefix('a').and_then(|n| n.parse::<u32>().ok()) {
                        Some(index) => {
                            self.to_phone(Message::NotificationAction { key, index });
                        }
                        None => self.send_ui("show", json!({ "page": "notifications" })),
                    }
                }
            }
            Event::ToastClosed { id, by_user } => {
                if let Some(key) = self.toasts.iter().find(|(_, v)| **v == id).map(|(k, _)| k.clone()) {
                    self.toasts.remove(&key);
                    if by_user && self.connected() {
                        self.to_phone(Message::DismissNotification { key });
                    }
                }
            }

            Event::TransferProgress { key, done } => {
                if let Some(t) = self.transfers.iter_mut().find(|t| t.key == key) {
                    // Smoothed, so the figure doesn't jump with every report.
                    let now = Instant::now();
                    if let Some((then, before)) = t.sample {
                        let seconds = now.duration_since(then).as_secs_f64();
                        if seconds > 0.05 && done > before {
                            let fresh = (done - before) as f64 / seconds;
                            t.speed = if t.speed > 0.0 { t.speed * 0.6 + fresh * 0.4 } else { fresh };
                        }
                    }
                    t.sample = Some((now, done));
                    t.eta = if t.speed > 0.0 && t.size > 0 { ((t.size as f64 - done as f64).max(0.0) / t.speed).round() as i64 } else { -1 };
                    t.done = done;
                    self.send_ui("transfers", json!(self.transfers));
                }
            }
            Event::TransferEnd { key, done, error } => {
                let Some(t) = self.transfers.iter_mut().find(|t| t.key == key) else { return };
                t.done = done;
                match error {
                    Some(error) => {
                        t.state = "failed";
                        t.error = error;
                    }
                    // A file sent to the phone is done when the phone says so.
                    None if t.incoming => t.state = "done",
                    None => {}
                }
                if t.state == "done" || t.state == "failed" {
                    t.speed = 0.0;
                    t.eta = -1;
                }
                if t.incoming && t.state == "done" {
                    crate::thumbs::make(t.key.clone(), std::path::PathBuf::from(&t.path), self.tx.clone());
                }
                let arrived = (t.incoming && t.state == "done").then(|| t.name.clone());
                self.send_ui("transfers", json!(self.transfers));
                self.save_history();
                if let (Some(name), Some(session), true) = (arrived, self.session.clone(), self.config.toasts) {
                    let toast = Toast {
                        replaces: 0,
                        app: "Omalink",
                        icon: "folder-download",
                        title: "File received from your phone",
                        body: &format!("{name} is in Downloads."),
                        actions: Vec::new(),
                        urgent: false,
                    };
                    desktop::notify(&session, toast).await;
                }
            }
            Event::NetReady(node) => {
                self.node = Some(node);
                self.net_state = "ready";
                self.net_error.clear();
                self.emit_net();
            }
            Event::NetFailed(error) => {
                self.net_state = "failed";
                self.net_error = error;
                self.emit_net();
            }
            Event::NetIncoming { alpn, conn } => {
                let id = omalink_net::remote(&conn);
                if omalink_net::is_pair(&alpn) {
                    // Only while pairing is open, and one phone at a time.
                    match self.pairing.as_mut() {
                        Some(p) if p.session.is_none() => {
                            p.session = Some(net::open(conn, true, self.tx.clone()));
                            p.phone_name.clear();
                            p.icon = None;
                            self.emit_net();
                        }
                        _ => conn.close(1u32.into(), b"not pairing"),
                    }
                } else if omalink_net::is_link(&alpn) && self.trust.contains(&id) {
                    // A phone reconnecting replaces its old connection.
                    let had_bluetooth = self.link.is_some();
                    self.net_link = Some(net::open(conn, false, self.tx.clone()));
                    self.net_route = "connecting".into();
                    if let Some(session) = &self.net_link {
                        session.send(Message::Hello { version: PROTOCOL_VERSION, name: hostname() });
                        if !had_bluetooth {
                            session.send(Message::Sync {
                                what: ["status", "permissions", "notifications", "conversations", "call_log", "contacts", "wallpaper"]
                                    .map(String::from)
                                    .to_vec(),
                            });
                        }
                    }
                    self.link_error.clear();
                    self.emit_net();
                    self.emit_link();
                    self.emit_mirror();
                } else {
                    conn.close(1u32.into(), b"not trusted");
                }
            }
            Event::NetDown { serial, reason } => {
                if self.pairing.as_ref().and_then(|p| p.session.as_ref()).is_some_and(|s| s.serial == serial) {
                    if let Some(p) = self.pairing.as_mut() {
                        p.session = None;
                        p.icon = None;
                    }
                    if self.verify.as_ref().is_some_and(|(s, _)| *s == serial) {
                        self.verify = None;
                    }
                    self.emit_net();
                } else if self.net_link.as_ref().is_some_and(|s| s.serial == serial) {
                    self.net_link = None;
                    self.net_route.clear();
                    if !matches!(self.mirror.state, "" | "idle" | "ended") {
                        self.end_mirror("The connection to the phone was lost.");
                    }
                    if self.verify.as_ref().is_some_and(|(s, _)| *s == serial) {
                        self.verify = None;
                    }
                    for t in self.transfers.iter_mut().filter(|t| t.via == "network" && (t.state == "waiting" || t.state == "active")) {
                        t.state = "failed";
                        t.error = reason.clone();
                    }
                    self.send_ui("transfers", json!(self.transfers));
                    self.save_history();
                    if self.link.is_none() {
                        self.link_error = reason;
                    }
                    self.emit_net();
                    self.emit_link();
                    self.emit_mirror();
                }
            }
            Event::NetPeer { serial, message } => {
                if self.handle_verify(serial, &message) {
                    return;
                }
                let on_pairing = self.pairing.as_ref().and_then(|p| p.session.as_ref()).is_some_and(|s| s.serial == serial);
                if on_pairing {
                    if let (Message::Hello { name, .. }, Some(p)) = (&message, self.pairing.as_mut()) {
                        p.phone_name = name.clone();
                        self.emit_net();
                    }
                } else if self.net_link.as_ref().is_some_and(|s| s.serial == serial) {
                    self.handle_peer(message).await;
                }
            }
            Event::NetRoute { serial, route } => {
                if self.net_link.as_ref().is_some_and(|s| s.serial == serial) {
                    self.net_route = route;
                    self.emit_net();
                }
            }
            Event::NetFileIncoming(transfer) => self.add_transfer(transfer),
            Event::MirrorLive { serial, width, height } => {
                if self.net_link.as_ref().is_some_and(|s| s.serial == serial) && self.mirror.state != "idle" {
                    self.mirror.state = "live";
                    self.mirror.width = width;
                    self.mirror.height = height;
                    self.mirror.reason.clear();
                    self.emit_mirror();
                    // Development builds can capture the live page, as the demo does.
                    #[cfg(debug_assertions)]
                    if let Ok(path) = std::env::var("OMALINK_SHOT") {
                        self.send_ui("shot", json!({ "path": path, "page": 7, "delay": 3000 }));
                    }
                }
            }
            Event::MirrorFps { serial, fps } => {
                if self.net_link.as_ref().is_some_and(|s| s.serial == serial) && self.mirror.state == "live" {
                    #[cfg(debug_assertions)]
                    eprintln!("omalink: mirroring {}x{} at {fps} fps", self.mirror.width, self.mirror.height);
                    self.mirror.fps = fps;
                    self.emit_mirror();
                }
            }
            Event::MirrorKeyframe { serial } => {
                if let Some(session) = self.net_link.as_ref().filter(|s| s.serial == serial) {
                    session.send(Message::KeyframeRequest);
                }
            }
            // A turned phone ends one stream and starts another at the new size.
            Event::MirrorStreamEnd { serial } => {
                if self.net_link.as_ref().is_some_and(|s| s.serial == serial) && self.mirror.state == "live" {
                    self.mirror.state = "approving";
                    self.emit_mirror();
                }
            }
            Event::MirrorFailed { serial, reason } => {
                if self.net_link.as_ref().is_some_and(|s| s.serial == serial) {
                    if let Some(session) = &self.net_link {
                        session.send(Message::MirrorStop);
                    }
                    self.end_mirror(&reason);
                }
            }

            Event::Thumb { key, path } => {
                if let Some(t) = self.transfers.iter_mut().find(|t| t.key == key) {
                    t.thumb = format!("file://{path}");
                    self.send_ui("transfers", json!(self.transfers));
                }
            }
            Event::Tick => {
                self.ticks += 1;
                if self.pairing.as_ref().is_some_and(|p| Instant::now() >= p.until) {
                    self.close_pairing();
                }
                if self.link.is_some() {
                    if self.last_heard.elapsed() > Duration::from_secs(45) {
                        self.drop_link("The phone stopped responding.");
                        self.retry_later();
                    } else if self.ticks % 5 == 0 {
                        self.to_phone(Message::Ping { n: self.ticks });
                    }
                    if self.ticks % 20 == 0 {
                        self.connect_calls();
                    }
                } else {
                    if self.connecting.is_some_and(|since| since.elapsed() > Duration::from_secs(45)) {
                        self.connecting = None;
                        self.link_error = "The phone didn't answer.".into();
                        self.retry_later();
                        self.emit_link();
                    }
                    if Instant::now() >= self.next_attempt {
                        self.try_connect();
                    }
                }
            }
        }
    }

    fn push_clip(&mut self, text: &str, from: &'static str) {
        self.clips.retain(|c| c.text != text);
        self.clips.insert(0, Clip { text: text.to_string(), time: now_ms(), from });
        self.clips.truncate(30);
        self.send_ui("clips", json!(self.clips));
        self.save_history();
    }

    async fn handle_peer(&mut self, message: Message) {
        match message {
            Message::Hello { version, name } => {
                self.peer_name = name;
                if version != PROTOCOL_VERSION {
                    self.note("The phone app and this app are different versions. Update both.");
                }
                self.emit_link();
            }
            Message::Ping { n } => {
                self.to_phone(Message::Pong { n });
            }
            Message::Pong { .. } => {}
            Message::Status(status) => {
                self.status = Some(status);
                self.send_ui("status", json!(self.status));
            }
            Message::Permissions { granted, missing } => {
                // Something was just granted: fetch what it unlocks.
                let newly: Vec<&String> = granted.iter().filter(|p| !self.granted.contains(p)).collect();
                if !self.granted.is_empty() || !self.missing.is_empty() {
                    let mut what = Vec::new();
                    for p in newly {
                        match p.as_str() {
                            permission::SMS => what.push("conversations".to_string()),
                            permission::CONTACTS => what.extend(["contacts".to_string(), "conversations".to_string()]),
                            permission::CALL_LOG => what.push("call_log".to_string()),
                            permission::NOTIFICATIONS => what.push("notifications".to_string()),
                            permission::WALLPAPER => what.push("wallpaper".to_string()),
                            _ => {}
                        }
                    }
                    if !what.is_empty() {
                        what.dedup();
                        self.to_phone(Message::Sync { what });
                    }
                }
                self.granted = granted;
                self.missing = missing;
                self.send_ui("permissions", json!({ "granted": self.granted, "missing": self.missing }));
            }
            Message::Notifications { mut items } => {
                items.iter_mut().for_each(Self::store_icon);
                items.sort_by_key(|n| -n.time);
                self.notifications = items;
                self.emit_notifications();
            }
            Message::NotificationPosted(mut n) => {
                Self::store_icon(&mut n);
                let previous = self.notifications.iter().position(|o| o.key == n.key);
                let changed = previous.is_none_or(|i| {
                    let old = &self.notifications[i];
                    old.title != n.title || old.text != n.text
                });
                if let Some(i) = previous {
                    self.notifications.remove(i);
                }
                // Ongoing notifications (music, navigation) update constantly;
                // they belong in the list but would be noise as popups.
                if changed && self.config.toasts && n.clearable {
                    if let Some(session) = self.session.clone() {
                        let icon = Self::icon_path(&n.package);
                        let mut actions = vec![("default".to_string(), "Open".to_string())];
                        actions.extend(n.actions.iter().enumerate().map(|(i, label)| (format!("a{i}"), label.clone())));
                        let id = desktop::notify(
                            &session,
                            Toast {
                                replaces: self.toasts.get(&n.key).copied().unwrap_or(0),
                                app: &n.app,
                                icon: if icon.exists() { icon.to_str().unwrap_or("phone") } else { "phone" },
                                title: if n.title.is_empty() { &n.app } else { &n.title },
                                body: &n.text,
                                actions,
                                urgent: false,
                            },
                        )
                        .await;
                        if let Some(id) = id {
                            self.toasts.insert(n.key.clone(), id);
                        }
                    }
                }
                self.notifications.insert(0, n);
                self.emit_notifications();
            }
            Message::NotificationRemoved { key } => {
                self.notifications.retain(|n| n.key != key);
                if let (Some(id), Some(session)) = (self.toasts.remove(&key), &self.session) {
                    desktop::close_notification(session, id).await;
                }
                self.emit_notifications();
            }
            Message::Conversations { items } => {
                self.conversations = items;
                // A thread on screen is being read as its messages arrive.
                let open = self.open_thread;
                self.apply_seen(open, true);
                self.send_ui("conversations", json!(self.conversations));
                // A message came or went while a thread is on screen.
                if let Some(thread_id) = self.open_thread {
                    self.to_phone(Message::GetMessages { thread_id, limit: 200 });
                }
            }
            Message::Messages { thread_id, items } => {
                self.send_ui("messages", json!({ "thread_id": thread_id, "items": items }));
            }
            Message::SmsSent { id, ok, error } => {
                self.send_ui("sms_sent", json!({ "id": id, "ok": ok, "error": error }));
                if !ok {
                    self.note(if error.is_empty() { "The message could not be sent.".to_string() } else { error });
                }
            }
            Message::CallLog { items } => {
                self.call_log = items;
                self.send_ui("call_log", json!(self.call_log));
            }
            Message::Contacts { items } => {
                self.names = items
                    .iter()
                    .flat_map(|c| c.numbers.iter().map(move |n| (number_key(n), c.name.clone())))
                    .collect();
                self.contacts = items;
                self.emit_contacts();
                // Pictures are fetched once and kept; ask only for new ones.
                let mut wanted: Vec<i64> =
                    self.contacts.iter().map(|c| c.photo).filter(|&id| id != 0 && !self.photo_path(id).exists()).collect();
                wanted.sort_unstable();
                wanted.dedup();
                if !wanted.is_empty() {
                    self.to_phone(Message::GetContactPhotos { ids: wanted });
                }
            }
            Message::ContactPhotos { items } => {
                for photo in &items {
                    Self::store_image(&self.photo_path(photo.id), &photo.data);
                }
                self.emit_contacts();
            }
            Message::FileOffer { id, name, size, port, addrs, key } => {
                let dest = transfer::destination(&name);
                let name_for_kind = name.clone();
                self.add_transfer(Transfer {
                    key: transfer::key(true, id),
                    name: dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(name),
                    size,
                    done: 0,
                    incoming: true,
                    state: "active",
                    error: String::new(),
                    path: dest.to_string_lossy().into_owned(),
                    time: now_ms(),
                    via: "bluetooth",
                    kind: crate::thumbs::kind(&name_for_kind),
                    eta: -1,
                    ..Default::default()
                });
                tokio::spawn(transfer::download(Self::route(addrs, port, &key), dest, id, self.tx.clone()));
            }
            Message::FileReady { id, port, addrs, key } => {
                let wanted = transfer::key(false, id);
                if let Some(t) = self.transfers.iter_mut().find(|t| t.key == wanted && t.state == "waiting") {
                    t.state = "active";
                    let source = std::path::PathBuf::from(&t.path);
                    tokio::spawn(transfer::upload(Self::route(addrs, port, &key), source, id, self.tx.clone()));
                    self.send_ui("transfers", json!(self.transfers));
                }
            }
            Message::FileSaved { id, ok, error } => {
                // The phone has the last word on a file sent to it.
                let wanted = transfer::key(false, id);
                if let Some(t) = self.transfers.iter_mut().find(|t| t.key == wanted) {
                    if ok {
                        t.state = "done";
                        t.done = t.size.max(0) as u64;
                    } else if t.state != "failed" {
                        t.state = "failed";
                        t.error = error;
                    }
                    self.send_ui("transfers", json!(self.transfers));
                    self.save_history();
                }
            }
            Message::Photos { items, more, cursor, before } => {
                let dir = self.phone_cache().join("gallery");
                let items: Vec<PhoneMedia> = items
                    .into_iter()
                    .map(|mut m| {
                        let path = dir.join(format!("{}.jpg", m.id));
                        if !m.thumb.is_empty() && (path.exists() || Self::store_image(&path, &m.thumb)) {
                            m.thumb = format!("file://{}", path.display());
                        } else {
                            m.thumb.clear();
                        }
                        m
                    })
                    .collect();
                if before == 0 {
                    self.gallery = items;
                } else {
                    let known: std::collections::HashSet<i64> = self.gallery.iter().map(|m| m.id).collect();
                    self.gallery.extend(items.into_iter().filter(|m| !known.contains(&m.id)));
                }
                self.gallery_more = more;
                self.gallery_cursor = cursor;
                self.gallery_loading = false;
                self.emit_gallery();
            }
            Message::PhotosChanged => {
                if !self.gallery.is_empty() {
                    self.load_gallery(false);
                }
            }
            Message::Wallpaper { image } => {
                if self.store_wallpaper(&image) {
                    self.emit_wallpaper();
                }
            }
            Message::CallState { phase, number, name } => {
                self.phone_call = (phase, number, name);
                self.emit_call().await;
            }
            Message::Clipboard { text } => {
                self.last_clip = text.clone();
                if desktop::write_clipboard(&text).await {
                    self.push_clip(&text, "phone");
                } else {
                    self.note("Couldn't write to the clipboard. Is wl-clipboard installed?");
                }
            }
            Message::Failed { what, reason } => {
                self.note(if reason.is_empty() { format!("The phone couldn't do that ({what}).") } else { reason });
            }
            Message::MirrorEnded { reason } => {
                if !matches!(self.mirror.state, "" | "idle") {
                    self.end_mirror(if reason.is_empty() { "The phone stopped sharing its screen." } else { &reason });
                }
            }
            Message::MirrorStop => {
                if !matches!(self.mirror.state, "" | "idle") {
                    self.end_mirror("The phone stopped sharing its screen.");
                }
            }
            Message::ControlState { available } => {
                self.mirror.control = available;
                self.emit_mirror();
            }
            // Things only a desktop says; a phone that sends them is confused.
            _ => {}
        }
    }

    async fn handle_ui(&mut self, name: &str, args: &Json) {
        if self.demo && crate::demo::handle(self, name, args) {
            return;
        }
        let system = self.system.clone();
        let adapter = self.bluez.adapter.as_ref().map(|a| a.path.clone());
        match name {
            "power" | "scan" => {
                let (Some(system), Some(adapter)) = (system, adapter) else {
                    return self.note("No Bluetooth adapter was found.");
                };
                let on = arg_bool(args, "on");
                let result = if name == "power" {
                    bluez::set_powered(&system, &adapter, on).await
                } else {
                    bluez::set_discovering(&system, &adapter, on).await
                };
                if let Err(e) = result {
                    // Stopping a scan that already ended is not worth a message.
                    if on {
                        self.note(e);
                    }
                }
            }
            "pair" => {
                let Some(system) = system else { return };
                let device = arg_str(args, "path");
                let tx = self.tx.clone();
                let label = self.bluez.devices.iter().find(|d| d.path == device).map(|d| d.name.clone()).unwrap_or_default();
                self.send_ui("pairing", json!({ "device": device, "name": label, "code": "", "confirm": false, "busy": true }));
                tokio::spawn(async move {
                    let result = bluez::pair(&system, &device).await;
                    let _ = tx.send(Event::PairDone { device, result });
                });
            }
            "pair_answer" => {
                if let Some(reply) = self.pending_pair.take() {
                    let _ = reply.send(arg_bool(args, "accept"));
                }
            }
            "use_device" => {
                self.config.device = arg_str(args, "address");
                self.config.save();
                self.link_error.clear();
                self.drop_link("");
                self.retry_now();
                self.emit_link();
            }
            "connect" => {
                self.link_error.clear();
                self.retry_now();
                self.emit_link();
            }
            "disconnect" => {
                self.paused = true;
                self.drop_link("");
                if let (Some(system), Some(device)) = (system, self.device()) {
                    let _ = bluez::disconnect(&system, &device.path.clone()).await;
                }
                self.emit_link();
            }
            "forget" => {
                let path = arg_str(args, "path");
                if self.bluez.devices.iter().any(|d| d.path == path && d.address == self.config.device) {
                    self.drop_link("");
                    self.config.device.clear();
                    self.config.save();
                }
                if let (Some(system), Some(adapter)) = (system, adapter) {
                    if let Err(e) = bluez::forget(&system, &adapter, &path).await {
                        self.note(e);
                    }
                }
                self.emit_link();
            }
            "connect_calls" => self.connect_calls(),
            "mirror_start" => {
                let Some(session) = &self.net_link else {
                    return self.note("Screen sharing needs the phone paired over the network. Pair it on the Phone page.");
                };
                session.send(Message::MirrorRequest { max_size: 1600 });
                self.mirror = Mirror { state: "requested", ..Default::default() };
                self.emit_mirror();
            }
            "mirror_stop" => {
                if let Some(session) = &self.net_link {
                    session.send(Message::MirrorStop);
                }
                self.end_mirror("");
                self.mirror.state = "idle";
                self.emit_mirror();
            }
            "mirror_input" => {
                let kind = match arg_str(args, "kind").as_str() {
                    "down" => InputKind::Down,
                    "move" => InputKind::Move,
                    "up" => InputKind::Up,
                    _ => return,
                };
                let at = |key: &str| args.get(key).and_then(Json::as_f64).unwrap_or(0.0).clamp(0.0, 1.0) as f32;
                self.to_mirror(Message::Input { kind, x: at("x"), y: at("y") });
            }
            "mirror_scroll" => {
                let at = |key: &str| args.get(key).and_then(Json::as_f64).unwrap_or(0.0) as f32;
                self.to_mirror(Message::Scroll { x: at("x").clamp(0.0, 1.0), y: at("y").clamp(0.0, 1.0), dx: at("dx").clamp(-10.0, 10.0), dy: at("dy").clamp(-10.0, 10.0) });
            }
            "mirror_action" => {
                let action = arg_str(args, "action");
                if matches!(action.as_str(), "back" | "home" | "recents" | "notifications" | "lock") {
                    self.to_mirror(Message::GlobalAction { action });
                }
            }
            "mirror_text" => {
                let text = arg_str(args, "text");
                if !text.is_empty() {
                    self.to_mirror(Message::TypeText { text });
                }
            }
            "mirror_key" => {
                let key = arg_str(args, "key");
                if matches!(key.as_str(), "enter" | "backspace" | "delete" | "left" | "right") {
                    self.to_mirror(Message::Key { key });
                }
            }
            "net_pair" => {
                if arg_bool(args, "on") {
                    self.open_pairing();
                } else {
                    self.close_pairing();
                }
            }
            "net_forget" => {
                let id = arg_str(args, "id");
                if self.trust.remove(&id) && self.net_link.as_ref().is_some_and(|s| s.id == id) {
                    self.net_link = None;
                    self.emit_link();
                }
                self.emit_net();
            }
            "net_relay" => {
                let relay = arg_str(args, "url").trim().to_string();
                if let Err(e) = omalink_net::relay_mode(&relay) {
                    return self.note(e);
                }
                self.config.relay = relay;
                self.config.save();
                // A relay is chosen when the node starts, so start it again.
                self.net_link = None;
                self.close_pairing();
                if let Some(node) = self.node.take() {
                    tokio::spawn(async move { node.close().await });
                }
                self.net_state = "starting";
                self.emit_net();
                self.emit_link();
                net::start(self.config.relay.clone(), self.tx.clone());
            }
            "show" => self.send_ui("show", json!({ "page": arg_str(args, "page") })),
            // The window is about to quit: say so, for whoever reads the status.
            "bye" => {
                let _ = std::fs::write(Self::status_path(), json!({ "running": false }).to_string());
            }

            "request_permissions" => {
                let permissions: Vec<String> = args
                    .get("permissions")
                    .and_then(Json::as_array)
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                if self.to_phone(Message::RequestPermissions { permissions }) {
                    self.note("Check your phone to allow access.");
                }
            }
            "sync" => {
                self.to_phone(Message::Sync {
                    what: ["status", "permissions", "notifications", "conversations", "call_log", "contacts", "wallpaper"]
                        .map(String::from)
                        .to_vec(),
                });
            }

            "open_thread" => {
                let thread_id = args.get("thread_id").and_then(Json::as_i64).unwrap_or(0);
                self.open_thread = Some(thread_id);
                if self.conversations.iter().any(|c| c.thread_id == thread_id && c.unread > 0) {
                    self.apply_seen(Some(thread_id), false);
                    self.send_ui("conversations", json!(self.conversations));
                }
                if self.connected() {
                    self.to_phone(Message::GetMessages { thread_id, limit: 200 });
                    self.to_phone(Message::MarkRead { thread_id });
                }
            }
            "close_thread" => self.open_thread = None,
            "send_sms" => {
                let (to, body) = (arg_str(args, "to"), arg_str(args, "body"));
                if to.trim().is_empty() || body.is_empty() {
                    return;
                }
                self.sms_id += 1;
                let id = self.sms_id;
                if self.to_phone(Message::SendSms { id, to, body }) {
                    self.send_ui("sms_queued", json!({ "id": id }));
                }
            }

            "dismiss" => {
                self.to_phone(Message::DismissNotification { key: arg_str(args, "key") });
            }
            "dismiss_all" => {
                let keys: Vec<String> = self.notifications.iter().filter(|n| n.clearable).map(|n| n.key.clone()).collect();
                for key in keys {
                    self.to_phone(Message::DismissNotification { key });
                }
            }
            "reply" => {
                self.to_phone(Message::ReplyNotification { key: arg_str(args, "key"), text: arg_str(args, "text") });
            }
            "action" => {
                let index = args.get("index").and_then(Json::as_u64).unwrap_or(0) as u32;
                self.to_phone(Message::NotificationAction { key: arg_str(args, "key"), index });
            }

            "dial" => {
                let number: String = arg_str(args, "number").chars().filter(|c| !c.is_whitespace()).collect();
                if number.is_empty() {
                    return;
                }
                match (&self.session, &self.telephony.modem) {
                    (Some(session), Some(modem)) => {
                        if let Err(e) = telephony::dial(session, modem, &number).await {
                            self.note(format!("Couldn't place the call: {e}"));
                        }
                    }
                    // Without hands-free the phone places the call itself and
                    // the audio stays on the phone.
                    _ => {
                        if self.to_phone(Message::Dial { number }) {
                            self.note("Calling from the phone. Call audio isn't connected to this computer.");
                        }
                    }
                }
            }
            "answer" => {
                let (_, _, path) = self.call_view();
                match (&self.session, path) {
                    (Some(session), Some(path)) => {
                        if let Err(e) = telephony::answer(session, &path).await {
                            self.note(format!("Couldn't answer: {e}"));
                        }
                    }
                    _ => {
                        self.to_phone(Message::AnswerCall);
                    }
                }
            }
            "hangup" => {
                let (_, _, path) = self.call_view();
                match (&self.session, path) {
                    (Some(session), Some(path)) => {
                        if let Err(e) = telephony::hangup(session, &path).await {
                            self.note(format!("Couldn't hang up: {e}"));
                        }
                    }
                    _ => {
                        self.to_phone(Message::EndCall);
                    }
                }
            }
            "tone" => {
                if let (Some(session), Some(modem)) = (&self.session, &self.telephony.modem) {
                    let _ = telephony::tones(session, modem, &arg_str(args, "tone")).await;
                }
            }
            "mute" => {
                self.muted = arg_bool(args, "on");
                desktop::set_mic_muted(self.muted).await;
                self.emit_call().await;
            }
            "send_files" => {
                let paths = args.get("paths").and_then(Json::as_array).cloned().unwrap_or_default();
                for path in paths.iter().filter_map(Json::as_str) {
                    // Dropped files arrive as URLs.
                    let path = std::path::PathBuf::from(path.strip_prefix("file://").unwrap_or(path));
                    let Some(size) = std::fs::metadata(&path).ok().filter(|m| m.is_file()).map(|m| m.len()) else {
                        self.note(format!("{} isn't a file that can be sent.", path.display()));
                        continue;
                    };
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    self.transfer_id += 1;
                    let id = self.transfer_id;
                    // The network link carries files itself; Bluetooth only arranges them.
                    let via = if let Some(session) = &self.net_link {
                        net::upload(session.conn.clone(), id, path.clone(), size as i64, self.tx.clone());
                        "network"
                    } else if self.to_phone(Message::SendFile { id, name: name.clone(), size: size as i64 }) {
                        "bluetooth"
                    } else {
                        break;
                    };
                    self.add_transfer(Transfer {
                        key: transfer::key(false, id),
                        name,
                        size: size as i64,
                        done: 0,
                        incoming: false,
                        state: if via == "network" { "active" } else { "waiting" },
                        error: String::new(),
                        path: path.to_string_lossy().into_owned(),
                        time: now_ms(),
                        via,
                        kind: crate::thumbs::kind(&path.to_string_lossy()),
                        eta: -1,
                        ..Default::default()
                    });
                    crate::thumbs::make(transfer::key(false, id), path.clone(), self.tx.clone());
                }
            }
            // Ctrl+V on the Files page: send what was copied, a file or an image.
            "paste_send" => match desktop::clipboard_files().await {
                Ok(paths) => {
                    let paths: Vec<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
                    Box::pin(self.handle_ui("send_files", &json!({ "paths": paths }))).await;
                }
                Err(e) => self.note(e),
            },
            "open_path" | "show_path" => {
                // Only files this app put there or was given, not any path.
                let path = arg_str(args, "path");
                if self.transfers.iter().any(|t| t.path == path) {
                    let target = if name == "show_path" {
                        std::path::Path::new(&path).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
                    } else {
                        path
                    };
                    let _ = tokio::process::Command::new("xdg-open").arg(target).spawn();
                }
            }
            "clear_transfers" => {
                self.transfers.retain(|t| t.state == "waiting" || t.state == "active");
                self.send_ui("transfers", json!(self.transfers));
                self.save_history();
            }
            "gallery_load" => self.load_gallery(arg_bool(args, "more")),
            "gallery_get" => {
                let ids: Vec<i64> = args.get("ids").and_then(Json::as_array).map(|a| a.iter().filter_map(Json::as_i64).collect()).unwrap_or_default();
                for id in ids.iter().take(200) {
                    self.to_phone(Message::GetPhoto { id: *id });
                }
                if !ids.is_empty() && self.connected() {
                    self.note(if ids.len() == 1 { "Downloading from your phone…".to_string() } else { format!("Downloading {} items from your phone…", ids.len()) });
                }
            }
            "gallery_delete" => {
                let ids: Vec<i64> = args.get("ids").and_then(Json::as_array).map(|a| a.iter().filter_map(Json::as_i64).collect()).unwrap_or_default();
                if !ids.is_empty() && self.to_phone(Message::DeletePhotos { ids: ids.clone() }) {
                    // Gone from here at once; the phone catches up once it's
                    // confirmed there. Cancelled ones come back on the next refresh.
                    self.gallery.retain(|m| !ids.contains(&m.id));
                    self.emit_gallery();
                    self.note("Confirm on your phone to delete.");
                }
            }
            "find_phone" => {
                self.to_phone(Message::FindPhone);
            }

            "send_clipboard" => {
                let text = arg_str(args, "text");
                if !text.is_empty() && self.to_phone(Message::Clipboard { text: text.clone() }) {
                    self.push_clip(&text, "desktop");
                }
            }
            "copy" => {
                let text = arg_str(args, "text");
                self.last_clip = text.clone();
                desktop::write_clipboard(&text).await;
            }
            "set" => {
                let on = arg_bool(args, "value");
                match arg_str(args, "key").as_str() {
                    "clipboard" => self.config.clipboard = on,
                    "toasts" => self.config.toasts = on,
                    "calls" => {
                        self.config.calls = on;
                        if on {
                            self.connect_calls();
                        }
                    }
                    _ => {}
                }
                self.config.save();
                self.emit_settings();
            }
            _ => {}
        }
    }
}

fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname").map(|s| s.trim().to_string()).unwrap_or_else(|_| "Desktop".into())
}

/// Lets a second launch raise the first one's window instead of starting again.
struct Instance {
    tx: UnboundedSender<Event>,
}

#[zbus::interface(name = "io.omalink.App")]
impl Instance {
    async fn show(&self) {
        let _ = self.tx.send(Event::Ui("show".into(), Json::Null));
    }

    /// Sends files to the phone, for scripts and file manager actions.
    async fn send(&self, paths: Vec<String>) {
        let _ = self.tx.send(Event::Ui("send_files".into(), json!({ "paths": paths })));
    }

    /// Starts sharing the phone's screen, for scripts and keybindings.
    async fn mirror(&self) {
        let _ = self.tx.send(Event::Ui("mirror_start".into(), Json::Null));
        let _ = self.tx.send(Event::Ui("show".into(), json!({ "page": "screen" })));
    }

    /// Opens pairing over the network, for scripts and keybindings.
    async fn pair(&self) {
        let _ = self.tx.send(Event::Ui("net_pair".into(), json!({ "on": true })));
        let _ = self.tx.send(Event::Ui("show".into(), Json::Null));
    }
}

pub async fn run(emit: Emit, tx: UnboundedSender<Event>, mut rx: UnboundedReceiver<Event>, demo: bool, background: bool) {
    let mut core = Core {
        emit,
        tx: tx.clone(),
        system: None,
        session: None,
        demo,
        started: Instant::now(),
        config: if demo { Config::default() } else { Config::load() },
        bluez: Default::default(),
        pending_pair: None,
        link: None,
        connecting: None,
        paused: false,
        next_attempt: Instant::now(),
        backoff: 0,
        link_error: String::new(),
        peer_name: String::new(),
        last_heard: Instant::now(),
        ticks: 0,
        telephony: Default::default(),
        phone_call: Default::default(),
        call_phase: "idle",
        call_since: 0,
        call_toast: None,
        muted: false,
        status: None,
        granted: Vec::new(),
        missing: Vec::new(),
        notifications: Vec::new(),
        toasts: HashMap::new(),
        conversations: Vec::new(),
        open_thread: None,
        contacts: Vec::new(),
        names: HashMap::new(),
        call_log: Vec::new(),
        clips: Vec::new(),
        last_clip: String::new(),
        sms_id: 0,
        transfers: Vec::new(),
        // Not from zero, so a restart can't reuse an id the phone still holds.
        transfer_id: now_ms() as u64,
        published: String::new(),
        node: None,
        net_state: if demo { "off" } else { "starting" },
        net_error: String::new(),
        trust: if demo { Trust::default() } else { Trust::load(net::trust_path()) },
        net_link: None,
        net_route: String::new(),
        pairing: None,
        verify: None,
        mirror: Mirror::default(),
        gallery: Vec::new(),
        gallery_cursor: 0,
        gallery_more: false,
        gallery_loading: false,
    };

    if !demo {
        let (transfers, clips) = History::load();
        core.transfers = transfers;
        core.clips = clips;
        core.make_thumbs();
    }

    tokio::spawn(desktop::watch_theme(tx.clone()));

    if demo {
        crate::demo::seed(&mut core);
        core.send_ui("ready", json!({ "show": true }));
    } else {
        if let Ok(session) = Connection::session().await {
            let taken = !matches!(
                session
                    .request_name_with_flags("io.omalink.App", zbus::fdo::RequestNameFlags::DoNotQueue.into())
                    .await,
                Ok(zbus::fdo::RequestNameReply::PrimaryOwner)
            );
            if taken {
                let _ = session
                    .call_method(Some("io.omalink.App"), "/io/omalink/App", Some("io.omalink.App"), "Show", &())
                    .await;
                core.send_ui("quit", Json::Null);
                return;
            }
            let _ = session.object_server().at("/io/omalink/App", Instance { tx: tx.clone() }).await;
            tokio::spawn(desktop::watch_notifications(session.clone(), tx.clone()));
            tokio::spawn(telephony::watch(session.clone(), tx.clone()));
            core.session = Some(session);
        }
        match Connection::system().await {
            Ok(system) => {
                if let Err(e) = bluez::register_agent(&system, tx.clone()).await {
                    core.note(format!("Pairing from this window is unavailable: {}", bluez::explain(e)));
                }
                if let Err(e) = link::register(&system, tx.clone()).await {
                    core.note(format!("Couldn't set up the phone connection: {}", bluez::explain(e)));
                }
                tokio::spawn(bluez::watch(system.clone(), tx.clone()));
                core.system = Some(system);
            }
            Err(e) => core.note(format!("Couldn't reach the system bus: {e}")),
        }
        tokio::spawn(desktop::watch_clipboard(tx.clone()));
        net::start(core.config.relay.clone(), tx.clone());
        let ticker = tx.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3));
            loop {
                interval.tick().await;
                if ticker.send(Event::Tick).is_err() {
                    return;
                }
            }
        });
        core.emit_settings();
        core.emit_link();
        core.emit_net();
        core.emit_mirror();
        core.emit_phone();
        core.send_ui("ready", json!({ "show": !background }));
    }

    core.publish();
    while let Some(event) = rx.recv().await {
        core.handle(event).await;
        core.publish();
    }
}
