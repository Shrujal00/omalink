//! The phone's half of the network link, in plain Rust so it can be tried on
//! a computer; `lib.rs` only wraps it for Kotlin.
//!
//! Kotlin drives it with a handful of calls and reads what happened as JSON
//! events from [`Phone::next_event`], on a thread of its own:
//!
//! - `{"ev":"found","id","name"}`, `{"ev":"gone","id"}`: computers that are
//!   pairing nearby, or that Bluetooth told us about.
//! - `{"ev":"link","connected","name","route"}`: the link came, went, or
//!   changed path.
//! - `{"ev":"message","json"}`: a message from the computer, as on Bluetooth.
//! - `{"ev":"verify","purpose","name","choices":[{"index","emoji","name"}]}`:
//!   show three icons; answer with [`Phone::choose`].
//! - `{"ev":"verify_done","purpose","ok","name","max_size"}`: how a
//!   verification ended. After a mirroring one that passed, Kotlin asks
//!   Android for the screen.
//! - `{"ev":"file","handle","id","name","size"}`: a file from the computer,
//!   read with [`Phone::read`].
//!
//! The icon verification itself lives here, not in Kotlin, so the phone app
//! only ever draws what it is told.

use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use n0_future::StreamExt;
use omalink_net::iroh::endpoint::{RecvStream, SendStream};
use omalink_net::{id_bytes, load_key, open_messages, random32, read_header, Conn, Found, Node, Peer, Role, Trust};
use omalink_proto::net::{VideoFrame, LINK_ALPN, PAIR_ALPN};
use omalink_proto::pair::{Chooser, Sas, ICONS, MIRROR, PAIR};
use omalink_proto::{Message, PROTOCOL_VERSION};
use serde_json::json;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::sync::Notify;

/// A verification waiting for the person to tap.
struct Verify {
    purpose: String,
    chooser: Chooser,
    sas: Option<Sas>,
    out: UnboundedSender<Message>,
    name: String,
    max_size: u32,
    /// The pairing session's desktop, to remember once it passes.
    desktop: String,
}

struct Link {
    conn: Conn,
    out: UnboundedSender<Message>,
    id: String,
    name: String,
}

enum Stream {
    Out(SendStream),
    In(RecvStream),
}

#[derive(Default)]
struct State {
    link: Option<Link>,
    verify: Option<Verify>,
    streams: HashMap<i64, Stream>,
    next_handle: i64,
    route: String,
}

pub struct Phone {
    rt: tokio::runtime::Runtime,
    node: Arc<Node>,
    name: String,
    trust: Arc<Mutex<Trust>>,
    state: Arc<Mutex<State>>,
    events: Sender<String>,
    queue: Mutex<Receiver<String>>,
    /// Wakes the link loop: a network change, a new pairing, a lost link.
    kick: Arc<Notify>,
}

fn icons(sas: &Sas) -> serde_json::Value {
    json!(sas.choices.iter().enumerate().map(|(i, &c)| json!({ "index": i, "emoji": ICONS[c].0, "name": ICONS[c].1 })).collect::<Vec<_>>())
}

impl Phone {
    /// Starts networking with the key and trusted computers kept in `dir`.
    pub fn start(dir: &Path, name: &str) -> Result<Self, String> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("omalink-net")
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let key = load_key(&dir.join("net-key"))?;
        let node = Arc::new(rt.block_on(Node::start(key, Role::Phone, ""))?);
        let (events, queue) = channel();
        let phone = Self {
            rt,
            node,
            name: name.to_string(),
            trust: Arc::new(Mutex::new(Trust::load(dir.join("peers.json")))),
            state: Arc::new(Mutex::new(State { next_handle: 1, ..Default::default() })),
            events,
            queue: Mutex::new(queue),
            kick: Arc::new(Notify::new()),
        };
        phone.spawn_browse();
        phone.spawn_link_loop();
        Ok(phone)
    }

    pub fn id(&self) -> String {
        self.node.id()
    }

    /// The next event, or `None` after `timeout`.
    pub fn next_event(&self, timeout: Duration) -> Option<String> {
        let queue = self.queue.lock().ok()?;
        match queue.recv_timeout(timeout) {
            Ok(event) => Some(event),
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => None,
        }
    }

    pub fn connected(&self) -> bool {
        self.state.lock().is_ok_and(|s| s.link.is_some())
    }

    /// Trusted computers, as JSON.
    pub fn peers(&self) -> String {
        let trust = self.trust.lock().unwrap();
        serde_json::to_string(&trust.peers).unwrap_or_else(|_| "[]".into())
    }

    pub fn forget(&self, id: &str) {
        self.trust.lock().unwrap().remove(id);
        let mut state = self.state.lock().unwrap();
        if state.link.as_ref().is_some_and(|l| l.id == id) {
            if let Some(link) = state.link.take() {
                link.conn.close(0u32.into(), b"forgotten");
            }
        }
    }

    /// The network changed (Wi-Fi to mobile data, say): look again now.
    pub fn network_changed(&self) {
        let node = self.node.clone();
        self.rt.spawn(async move { node.endpoint.network_change().await });
        self.kick.notify_one();
    }

    /// A computer this phone heard about over Bluetooth.
    pub fn invited(&self, id: &str, name: &str) {
        if omalink_net::parse_id(id).is_ok() {
            let _ = self.events.send(json!({ "ev": "found", "id": id, "name": name, "via": "bluetooth" }).to_string());
        }
    }

    /// Sends a message, as JSON, to the computer. Refuses JSON that isn't a
    /// message the computer would understand.
    pub fn send(&self, json: &str) -> bool {
        let Ok(message) = serde_json::from_str::<Message>(json) else { return false };
        let state = self.state.lock().unwrap();
        match &state.link {
            Some(link) => link.out.send(message).is_ok(),
            None => false,
        }
    }

    fn spawn_browse(&self) {
        let node = self.node.clone();
        let events = self.events.clone();
        self.rt.spawn(async move {
            let mut found = node.browse().await;
            while let Some(event) = found.next().await {
                let event = match event {
                    Found::Open { id, name } => json!({ "ev": "found", "id": id, "name": name, "via": "network" }),
                    Found::Gone { id } => json!({ "ev": "gone", "id": id }),
                };
                if events.send(event.to_string()).is_err() {
                    return;
                }
            }
        });
    }

    /// Keeps one link to a trusted computer up whenever there is one.
    fn spawn_link_loop(&self) {
        let node = self.node.clone();
        let trust = self.trust.clone();
        let state = self.state.clone();
        let events = self.events.clone();
        let kick = self.kick.clone();
        let name = self.name.clone();
        self.rt.spawn(async move {
            let mut backoff = 2u64;
            loop {
                let up = state.lock().unwrap().link.as_ref().map(|l| l.conn.clone());
                match up {
                    Some(conn) => {
                        tokio::select! {
                            _ = conn.closed() => {}
                            _ = kick.notified() => {}
                        }
                        continue;
                    }
                    None => {
                        let peers = trust.lock().unwrap().peers.clone();
                        let mut linked = false;
                        for peer in peers {
                            if let Ok(conn) = node.dial_peer(&peer, LINK_ALPN).await {
                                if let Some(addr) = node.known_addr(&peer.id).await {
                                    trust.lock().unwrap().update_addr(&peer.id, addr);
                                }
                                let local = id_bytes(&node.endpoint.id());
                                linked = connect(conn, peer, local, &name, &state, &events, &kick).await;
                                if linked {
                                    break;
                                }
                            }
                        }
                        if linked {
                            backoff = 2;
                            continue;
                        }
                    }
                }
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(backoff)) => {}
                    _ = kick.notified() => {}
                }
                backoff = (backoff * 2).min(30);
            }
        });
    }

    /// Pairs with a computer that is showing its pairing screen. The icons to
    /// offer arrive as a `verify` event.
    pub fn pair(&self, id: &str, name: &str) {
        let node = self.node.clone();
        let state = self.state.clone();
        let trust = self.trust.clone();
        let events = self.events.clone();
        let kick = self.kick.clone();
        let (id, desktop_name, own_name) = (id.to_string(), name.to_string(), self.name.clone());
        self.rt.spawn(async move {
            let fail = |reason: &str| {
                let _ = events.send(json!({ "ev": "verify_done", "purpose": PAIR, "ok": false, "name": desktop_name, "reason": reason }).to_string());
            };
            let conn = match node.dial(&id, PAIR_ALPN).await {
                Ok(conn) => conn,
                Err(e) => return fail(&e),
            };
            let hello = Message::Hello { version: PROTOCOL_VERSION, name: own_name };
            let (mut send, mut reader) = match open_messages(&conn, &hello).await {
                Ok(pair) => pair,
                Err(_) => return fail("That computer isn't pairing right now."),
            };
            let (out, mut queue) = unbounded_channel::<Message>();
            let chooser = match Chooser::new(PAIR, &id_bytes(&conn.remote_id()), &id_bytes(&node.endpoint.id()), &random32()) {
                Ok(chooser) => chooser,
                Err(_) => return fail("Couldn't start pairing."),
            };
            let _ = out.send(Message::VerifyCommit { purpose: PAIR.into(), commit: chooser.commitment() });
            state.lock().unwrap().verify = Some(Verify {
                purpose: PAIR.into(),
                chooser,
                sas: None,
                out,
                name: desktop_name.clone(),
                max_size: 0,
                desktop: id.clone(),
            });
            let writer = tokio::spawn(async move {
                while let Some(message) = queue.recv().await {
                    if omalink_net::write(&mut send, &message).await.is_err() {
                        break;
                    }
                }
            });
            // The person has a minute to look and tap.
            let talk = async {
                while let Ok(Some(message)) = reader.next().await {
                    if let Some(finished) = on_verify(&message, &state, &events) {
                        return Some(finished);
                    }
                }
                None
            };
            let result = tokio::time::timeout(Duration::from_secs(90), talk).await.ok().flatten();
            writer.abort();
            conn.close(0u32.into(), b"done");
            match result {
                Some(true) => {
                    // Where the computer was just reached makes the first link quick.
                    let addr = node.known_addr(&id).await;
                    trust.lock().unwrap().add(Peer { id, name: desktop_name, added: now_ms(), addr });
                    kick.notify_one();
                }
                Some(false) => {}
                None => {
                    if state.lock().unwrap().verify.take().is_some() {
                        fail("Pairing timed out.");
                    }
                }
            }
        });
    }

    /// The person tapped choice `index` (0, 1 or 2) of the icons on screen.
    pub fn choose(&self, index: usize) {
        let state = self.state.lock().unwrap();
        let Some(verify) = &state.verify else { return };
        let Some(sas) = verify.sas else { return };
        let Some(&tapped) = sas.choices.get(index) else { return };
        let _ = verify.out.send(Message::VerifyResult { ok: verify.chooser.check(tapped) });
    }

    /// The person declined on the phone.
    pub fn decline(&self) {
        if let Some(verify) = self.state.lock().unwrap().verify.take() {
            let _ = verify.out.send(Message::VerifyResult { ok: false });
            if verify.purpose == MIRROR {
                let _ = verify.out.send(Message::MirrorEnded { reason: "Declined on the phone.".into() });
            }
        }
    }

    // Streams, for Kotlin to fill and drain.

    fn store(&self, stream: Stream) -> i64 {
        let mut state = self.state.lock().unwrap();
        let handle = state.next_handle;
        state.next_handle += 1;
        state.streams.insert(handle, stream);
        handle
    }

    fn link_conn(&self) -> Option<Conn> {
        self.state.lock().unwrap().link.as_ref().map(|l| l.conn.clone())
    }

    /// Opens a stream for a file going to the computer; 0 when there's no link.
    pub fn open_file(&self, id: u64, name: &str, size: i64) -> i64 {
        let Some(conn) = self.link_conn() else { return 0 };
        let header = Message::FileStart { id, name: name.to_string(), size };
        match self.rt.block_on(open_uni(&conn, &header)) {
            Some(send) => self.store(Stream::Out(send)),
            None => 0,
        }
    }

    /// Opens the video stream of a mirroring session.
    pub fn open_mirror(&self, width: u32, height: u32) -> i64 {
        let Some(conn) = self.link_conn() else { return 0 };
        match self.rt.block_on(open_uni(&conn, &Message::MirrorStream { width, height })) {
            Some(send) => {
                // Behind files and messages, so a big file doesn't stall
                // the screen and the screen doesn't stall a reply.
                let _ = send.set_priority(-1);
                self.store(Stream::Out(send))
            }
            None => 0,
        }
    }

    pub fn write(&self, handle: i64, bytes: &[u8]) -> bool {
        let Some(Stream::Out(mut send)) = self.take(handle) else { return false };
        let ok = self.rt.block_on(send.write_all(bytes)).is_ok();
        self.put(handle, Stream::Out(send));
        ok
    }

    /// One encoded video frame. Returns false when the stream is gone.
    pub fn write_frame(&self, handle: i64, bytes: &[u8], pts_us: u64, flags: u8) -> bool {
        let Some(Stream::Out(mut send)) = self.take(handle) else { return false };
        let header = VideoFrame::header(bytes.len(), pts_us, flags);
        let ok = self.rt.block_on(async { send.write_all(&header).await.is_ok() && send.write_all(bytes).await.is_ok() });
        self.put(handle, Stream::Out(send));
        ok
    }

    /// Ends an outgoing stream; for a file, waits until the computer has it.
    pub fn finish(&self, handle: i64) -> bool {
        let Some(stream) = self.take(handle) else { return false };
        match stream {
            Stream::Out(mut send) => {
                if send.finish().is_err() {
                    return false;
                }
                self.rt.block_on(async { tokio::time::timeout(Duration::from_secs(60), send.stopped()).await.is_ok() })
            }
            Stream::In(_) => true,
        }
    }

    /// Reads from an incoming file: the byte count, 0 at the end, -1 on error.
    pub fn read(&self, handle: i64, buffer: &mut [u8]) -> i32 {
        let Some(Stream::In(mut recv)) = self.take(handle) else { return -1 };
        let n = match self.rt.block_on(recv.read(buffer)) {
            Ok(Some(n)) => n as i32,
            Ok(None) => 0,
            Err(_) => -1,
        };
        if n > 0 {
            self.put(handle, Stream::In(recv));
        }
        n
    }

    pub fn close(&self, handle: i64) {
        if let Some(Stream::In(mut recv)) = self.take(handle) {
            let _ = recv.stop(0u32.into());
        }
    }

    fn take(&self, handle: i64) -> Option<Stream> {
        self.state.lock().unwrap().streams.remove(&handle)
    }

    fn put(&self, handle: i64, stream: Stream) {
        self.state.lock().unwrap().streams.insert(handle, stream);
    }
}

async fn open_uni(conn: &Conn, header: &Message) -> Option<SendStream> {
    let mut send = conn.open_uni().await.ok()?;
    omalink_net::write(&mut send, header).await.ok()?;
    Some(send)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Takes the computer's side of a verification. Returns `Some(passed)` once
/// it is over, `None` for anything else.
fn on_verify(message: &Message, state: &Mutex<State>, events: &Sender<String>) -> Option<bool> {
    let mut state = state.lock().unwrap();
    match message {
        Message::VerifyNonce { nonce } => {
            let verify = state.verify.as_mut()?;
            match verify.chooser.nonce(nonce) {
                Ok((reveal, sas)) => {
                    verify.sas = Some(sas);
                    let _ = verify.out.send(Message::VerifyReveal { nonce: reveal });
                    let _ = events.send(
                        json!({ "ev": "verify", "purpose": verify.purpose, "name": verify.name, "choices": icons(&sas) }).to_string(),
                    );
                    None
                }
                Err(_) => {
                    let _ = verify.out.send(Message::VerifyResult { ok: false });
                    None
                }
            }
        }
        Message::VerifyDone { ok } => {
            let verify = state.verify.take()?;
            let _ = events.send(
                json!({
                    "ev": "verify_done",
                    "purpose": verify.purpose,
                    "ok": ok,
                    "name": verify.name,
                    "id": verify.desktop,
                    "max_size": verify.max_size,
                    "reason": if *ok { "" } else { "The icon didn't match. Nothing was shared." },
                })
                .to_string(),
            );
            Some(*ok)
        }
        _ => None,
    }
}

/// Sets up a link the phone dialed. False if the computer turned it away.
async fn connect(
    conn: Conn,
    peer: Peer,
    local: [u8; 32],
    name: &str,
    state: &Arc<Mutex<State>>,
    events: &Sender<String>,
    kick: &Arc<Notify>,
) -> bool {
    let hello = Message::Hello { version: PROTOCOL_VERSION, name: name.to_string() };
    let Ok((mut send, mut reader)) = open_messages(&conn, &hello).await else { return false };
    let (out, mut queue) = unbounded_channel::<Message>();
    tokio::spawn(async move {
        while let Some(message) = queue.recv().await {
            if omalink_net::write(&mut send, &message).await.is_err() {
                break;
            }
        }
    });
    {
        let mut s = state.lock().unwrap();
        s.link = Some(Link { conn: conn.clone(), out: out.clone(), id: peer.id.clone(), name: peer.name.clone() });
        s.route = omalink_net::route(&conn).to_string();
    }
    let _ = events.send(json!({ "ev": "link", "connected": true, "name": peer.name, "route": omalink_net::route(&conn) }).to_string());

    // Messages from the computer.
    let (reading, st, ev, k, peer_name) = (conn.clone(), state.clone(), events.clone(), kick.clone(), peer.name.clone());
    tokio::spawn(async move {
        loop {
            match reader.next().await {
                Ok(Some(message)) => {
                    if on_verify(&message, &st, &ev).is_some() {
                        continue;
                    }
                    match message {
                        Message::MirrorRequest { max_size } => {
                            // The verification starts here; the person
                            // decides on the phone.
                            let chooser = Chooser::new(MIRROR, &id_bytes(&reading.remote_id()), &local, &random32());
                            if let Ok(chooser) = chooser {
                                let mut s = st.lock().unwrap();
                                let out = s.link.as_ref().map(|l| l.out.clone());
                                if let Some(out) = out {
                                    let _ = out.send(Message::VerifyCommit { purpose: MIRROR.into(), commit: chooser.commitment() });
                                    s.verify = Some(Verify {
                                        purpose: MIRROR.into(),
                                        chooser,
                                        sas: None,
                                        out,
                                        name: peer_name.clone(),
                                        max_size,
                                        desktop: String::new(),
                                    });
                                }
                            }
                        }
                        other => {
                            let json = serde_json::to_string(&other).unwrap_or_default();
                            let _ = ev.send(json!({ "ev": "message", "json": json }).to_string());
                        }
                    }
                }
                Ok(None) | Err(_) => break,
            }
        }
        let mut s = st.lock().unwrap();
        if s.link.as_ref().is_some_and(|l| l.conn.stable_id() == reading.stable_id()) {
            s.link = None;
            s.verify = None;
            drop(s);
            let _ = ev.send(json!({ "ev": "link", "connected": false }).to_string());
        }
        k.notify_one();
    });

    // Files from the computer.
    let (streams, st, ev) = (conn.clone(), state.clone(), events.clone());
    tokio::spawn(async move {
        while let Ok(mut recv) = streams.accept_uni().await {
            if let Ok(Message::FileStart { id, name, size }) = read_header(&mut recv).await {
                let handle = {
                    let mut s = st.lock().unwrap();
                    let handle = s.next_handle;
                    s.next_handle += 1;
                    s.streams.insert(handle, Stream::In(recv));
                    handle
                };
                let _ = ev.send(json!({ "ev": "file", "handle": handle, "id": id, "name": name, "size": size }).to_string());
            }
        }
    });

    // Which way the bytes go.
    let (watched, st, ev) = (conn, state.clone(), events.clone());
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = watched.closed() => return,
                _ = tokio::time::sleep(Duration::from_secs(3)) => {}
            }
            let now = omalink_net::route(&watched).to_string();
            let mut s = st.lock().unwrap();
            if s.route != now && s.link.as_ref().is_some_and(|l| l.conn.stable_id() == watched.stable_id()) {
                s.route = now.clone();
                let name = s.link.as_ref().map(|l| l.name.clone()).unwrap_or_default();
                let _ = ev.send(json!({ "ev": "link", "connected": true, "name": name, "route": now }).to_string());
            }
        }
    });
    true
}
