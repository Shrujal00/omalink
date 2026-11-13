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
    number_key, permission, CallEntry, CallPhase, Contact, Conversation, Message, Notification, PhoneStatus,
    PROTOCOL_VERSION, SERVICE_UUID,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
use zbus::Connection;

use crate::desktop::{self, Toast};
use crate::link::Link;
use crate::{bluez, link, telephony, transfer};

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
    Tick,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    /// Bluetooth address of the phone this desktop is linked to.
    pub device: String,
    pub clipboard: bool,
    pub toasts: bool,
    pub calls: bool,
    /// Thread → time of its newest message when it was last opened here.
    /// The phone can't always be told a thread was read, so the desktop
    /// remembers for itself.
    pub seen: HashMap<i64, i64>,
}

impl Default for Config {
    fn default() -> Self {
        Self { device: String::new(), clipboard: true, toasts: true, calls: true, seen: HashMap::new() }
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

/// A file going to or coming from the phone, as the Files page lists it.
#[derive(Serialize, Clone)]
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

    fn to_phone(&self, message: Message) -> bool {
        match &self.link {
            Some(link) => {
                link.send(message);
                true
            }
            None => {
                self.note("The phone isn't connected.");
                false
            }
        }
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
        let state = if self.link.is_some() {
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
        self.send_ui(
            "link",
            json!({
                "state": state,
                "address": self.config.device,
                "name": name,
                "error": if self.link.is_some() { "" } else { self.link_error.as_str() },
                "calls": self.telephony.modem.is_some(),
            }),
        );
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

    fn wallpaper_path(&self) -> std::path::PathBuf {
        self.phone_cache().join("wallpaper.jpg")
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
    fn add_transfer(&mut self, transfer: Transfer) {
        self.transfers.insert(0, transfer);
        self.transfers.truncate(50);
        self.send_ui("transfers", json!(self.transfers));
    }

    fn route(addrs: Vec<String>, port: u16, key: &str) -> transfer::Route {
        let key = base64::engine::general_purpose::STANDARD.decode(key.as_bytes()).unwrap_or_default();
        transfer::Route { addrs, port, key }
    }

    fn emit_wallpaper(&self) {
        let path = self.wallpaper_path();
        let shown = !self.demo && path.exists();
        self.send_ui("wallpaper", json!(if shown { format!("file://{}", path.display()) } else { String::new() }));
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
            "state": if self.link.is_some() { "connected" } else if self.config.device.is_empty() { "none" } else { "waiting" },
            "name": self.device().map(|d| d.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| self.peer_name.clone()),
            "battery": self.status.as_ref().filter(|_| self.link.is_some()).map(|s| s.battery).unwrap_or(-1),
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
            // Whatever the phone hadn't answered yet, it won't now.
            for t in self.transfers.iter_mut().filter(|t| t.state == "waiting") {
                t.state = "failed";
                t.error = "The phone disconnected.".into();
            }
            self.send_ui("transfers", json!(self.transfers));
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
                if self.config.clipboard && self.link.is_some() {
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
                    if by_user && self.link.is_some() {
                        self.to_phone(Message::DismissNotification { key });
                    }
                }
            }

            Event::TransferProgress { key, done } => {
                if let Some(t) = self.transfers.iter_mut().find(|t| t.key == key) {
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
                let arrived = (t.incoming && t.state == "done").then(|| t.name.clone());
                self.send_ui("transfers", json!(self.transfers));
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
            Event::Tick => {
                self.ticks += 1;
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
                }
            }
            Message::Wallpaper { image } => {
                if Self::store_image(&self.wallpaper_path(), &image) {
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
            "show" => self.send_ui("show", json!({})),
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
                if self.link.is_some() {
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
                    if !self.to_phone(Message::SendFile { id, name: name.clone(), size: size as i64 }) {
                        break;
                    }
                    self.add_transfer(Transfer {
                        key: transfer::key(false, id),
                        name,
                        size: size as i64,
                        done: 0,
                        incoming: false,
                        state: "waiting",
                        error: String::new(),
                        path: path.to_string_lossy().into_owned(),
                        time: now_ms(),
                    });
                }
            }
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
    };

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
        core.emit_phone();
        core.send_ui("ready", json!({ "show": !background }));
    }

    core.publish();
    while let Some(event) = rx.recv().await {
        core.handle(event).await;
        core.publish();
    }
}
