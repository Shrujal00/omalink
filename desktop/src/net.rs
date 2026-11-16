//! The network link to the phone, next to the Bluetooth one.
//!
//! It works wherever both devices have internet, not only in Bluetooth range:
//! on the same Wi-Fi the bytes go straight across, elsewhere straight to this
//! computer's public address when the router lets a port be opened, and
//! through an encrypted relay when nothing else works. The protocol is in
//! `omalink_proto::net`; finding the way is `omalink_net`'s.
//!
//! A session is one connection from the phone: the pairing one, which only
//! carries icon verification, or the link one, which carries the same
//! messages as Bluetooth plus files and the mirrored screen.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use omalink_net::{accept_messages, read_header, receive_file, route, send_file, Conn, Node, Role};
use omalink_proto::Message;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use crate::core::{now_ms, Event, Transfer};
use crate::transfer;

fn state_dir() -> PathBuf {
    let base = std::env::var("XDG_STATE_HOME")
        .unwrap_or_else(|_| format!("{}/.local/state", std::env::var("HOME").unwrap_or_default()));
    PathBuf::from(base).join("omalink")
}

pub fn key_path() -> PathBuf {
    state_dir().join("net-key")
}

pub fn trust_path() -> PathBuf {
    state_dir().join("peers.json")
}

/// Starts this computer's network node in the background.
pub fn start(relay: String, tx: UnboundedSender<Event>) {
    tokio::spawn(async move {
        let node = match omalink_net::load_key(&key_path()) {
            Ok(key) => Node::start(key, Role::Desktop, &relay).await,
            Err(e) => Err(e),
        };
        let node = match node {
            Ok(node) => Arc::new(node),
            Err(e) => {
                let _ = tx.send(Event::NetFailed(e));
                return;
            }
        };
        let _ = tx.send(Event::NetReady(node.clone()));
        while let Some(incoming) = node.incoming().await {
            // Each handshake on its own task, so one that never finishes
            // can't keep the phone waiting behind it.
            let tx = tx.clone();
            tokio::spawn(async move {
                // A failed handshake is someone else's problem.
                if let Ok((alpn, conn)) = omalink_net::handshake(incoming).await {
                    let _ = tx.send(Event::NetIncoming { alpn, conn });
                }
            });
        }
    });
}

static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// One connection from the phone. Dropping it hangs up.
pub struct Session {
    pub serial: u64,
    pub id: String,
    pub conn: Conn,
    out: UnboundedSender<Message>,
}

impl Session {
    pub fn send(&self, message: Message) {
        let _ = self.out.send(message);
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.conn.close(0u32.into(), b"bye");
    }
}

/// Hangs up once the other side has read what was sent, or after a few
/// seconds, so the last message of a conversation isn't thrown away.
pub fn retire(session: Session) {
    tokio::spawn(async move {
        let _ = tokio::time::timeout(Duration::from_secs(5), session.conn.closed()).await;
        drop(session);
    });
}

/// Takes over a connection the phone opened. Messages arrive as
/// `Event::NetPeer`; files and video as their own events.
pub fn open(conn: Conn, pair: bool, tx: UnboundedSender<Event>) -> Session {
    let serial = SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let (out, mut queue) = unbounded_channel::<Message>();
    let id = omalink_net::remote(&conn);

    let messages = conn.clone();
    let events = tx.clone();
    tokio::spawn(async move {
        let reason = match tokio::time::timeout(Duration::from_secs(20), accept_messages(&messages)).await {
            Err(_) => "The phone didn't say anything.".to_string(),
            Ok(Err(e)) => e,
            Ok(Ok((mut send, mut reader))) => {
                tokio::spawn(async move {
                    while let Some(message) = queue.recv().await {
                        if omalink_net::write(&mut send, &message).await.is_err() {
                            break;
                        }
                    }
                    let _ = send.finish();
                });
                loop {
                    match reader.next().await {
                        Ok(Some(message)) => {
                            if events.send(Event::NetPeer { serial, message }).is_err() {
                                return;
                            }
                        }
                        Ok(None) => break "The phone closed the connection.".to_string(),
                        Err(e) => break e,
                    }
                }
            }
        };
        let _ = events.send(Event::NetDown { serial, reason });
    });

    if !pair {
        let streams = conn.clone();
        let events = tx.clone();
        let replies = out.clone();
        tokio::spawn(async move {
            while let Ok(recv) = streams.accept_uni().await {
                tokio::spawn(stream(recv, serial, events.clone(), replies.clone()));
            }
        });

        // Which way the bytes are going, for the window.
        let watched = conn.clone();
        let events = tx;
        tokio::spawn(async move {
            let mut last = "";
            loop {
                let now = route(&watched);
                if now != last {
                    last = now;
                    if events.send(Event::NetRoute { serial, route: now.to_string() }).is_err() {
                        return;
                    }
                }
                tokio::select! {
                    _ = watched.closed() => return,
                    _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                }
            }
        });
    }

    Session { serial, id, conn, out }
}

/// A unidirectional stream from the phone: a file, or the mirrored screen.
async fn stream(mut recv: omalink_net::iroh::endpoint::RecvStream, serial: u64, tx: UnboundedSender<Event>, out: UnboundedSender<Message>) {
    match read_header(&mut recv).await {
        Ok(Message::FileStart { id, name, size }) => {
            let dest = transfer::destination(&name);
            let key = transfer::key(true, id);
            let _ = tx.send(Event::NetFileIncoming(Transfer {
                key: key.clone(),
                name: dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(name),
                size,
                done: 0,
                incoming: true,
                state: "active",
                error: String::new(),
                path: dest.to_string_lossy().into_owned(),
                time: now_ms(),
                via: "network",
                kind: crate::thumbs::kind(&dest.to_string_lossy()),
                eta: -1,
                ..Default::default()
            }));
            let mut part = dest.clone().into_os_string();
            part.push(".part");
            let part = PathBuf::from(part);
            let mut done = 0;
            let result = async {
                let file = tokio::fs::File::create(&part).await.map_err(|e| format!("Couldn't write {}: {e}", part.display()))?;
                let progress = tx.clone();
                let progress_key = key.clone();
                done = receive_file(&mut recv, file, size, move |n| {
                    let _ = progress.send(Event::TransferProgress { key: progress_key.clone(), done: n });
                })
                .await?;
                tokio::fs::rename(&part, &dest).await.map_err(|e| format!("Couldn't save the file: {e}"))
            }
            .await;
            if result.is_err() {
                let _ = tokio::fs::remove_file(&part).await;
            }
            let _ = out.send(Message::FileSaved { id, ok: result.is_ok(), error: result.clone().err().unwrap_or_default() });
            let _ = tx.send(Event::TransferEnd { key, done, error: result.err() });
        }
        Ok(Message::MirrorStream { width, height }) => {
            crate::video::receive(recv, width, height, serial, tx).await;
        }
        // Something from a newer phone app, or garbage: drop the stream.
        _ => {
            let _ = recv.stop(0u32.into());
        }
    }
}

/// Sends a file to the phone on its own stream. The phone has the last word,
/// with `FileSaved` on the message stream.
pub fn upload(conn: Conn, id: u64, source: PathBuf, size: i64, tx: UnboundedSender<Event>) {
    tokio::spawn(async move {
        let key = transfer::key(false, id);
        let name = source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
        let result = match tokio::fs::File::open(&source).await {
            Err(e) => Err(format!("Couldn't read {}: {e}", source.display())),
            Ok(file) => {
                let progress = tx.clone();
                let progress_key = key.clone();
                send_file(&conn, id, &name, size, file, move |n| {
                    let _ = progress.send(Event::TransferProgress { key: progress_key.clone(), done: n });
                })
                .await
            }
        };
        let (done, error) = match result {
            Ok(done) => (done, None),
            Err(e) => (0, Some(e)),
        };
        let _ = tx.send(Event::TransferEnd { key, done, error });
    });
}
