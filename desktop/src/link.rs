//! The data channel to the phone: an RFCOMM stream that BlueZ opens for us.
//!
//! We register a profile for our service UUID; asking BlueZ to connect that
//! profile on a device makes it look the service up on the phone, open the
//! channel, and hand us the socket through `NewConnection`.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::mpsc as std_mpsc;

use omalink_proto::{encode, Codec, DecodeError, Message, SERVICE_UUID};
use tokio::sync::mpsc::UnboundedSender;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};
use zbus::Connection;

use crate::core::Event;

const PROFILE_PATH: &str = "/omalink/profile";

/// The sending half of a live connection. Dropping it closes the socket.
pub struct Link {
    pub device: String,
    /// Tells this connection's events apart from a predecessor's.
    pub serial: u64,
    out: std_mpsc::Sender<Message>,
    socket: File,
}

impl Link {
    pub fn send(&self, message: Message) {
        let _ = self.out.send(message);
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        // Wakes the reader thread out of its blocking read.
        unsafe { libc::shutdown(self.socket.as_raw_fd(), libc::SHUT_RDWR) };
    }
}

struct Profile {
    tx: UnboundedSender<Event>,
}

#[zbus::interface(name = "org.bluez.Profile1")]
impl Profile {
    async fn release(&self) {}

    async fn new_connection(&self, device: ObjectPath<'_>, fd: zbus::zvariant::OwnedFd, _properties: HashMap<String, OwnedValue>) {
        let fd: OwnedFd = fd.into();
        match open(fd, device.to_string(), self.tx.clone()) {
            Ok(link) => {
                let _ = self.tx.send(Event::LinkUp(link));
            }
            Err(e) => {
                let _ = self.tx.send(Event::LinkDown { serial: 0, reason: e.to_string() });
            }
        }
    }

    async fn request_disconnection(&self, _device: ObjectPath<'_>) {}
}

static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn open(fd: OwnedFd, device: String, tx: UnboundedSender<Event>) -> std::io::Result<Link> {
    // BlueZ hands the socket over non-blocking; plain threads want it blocking.
    unsafe {
        let flags = libc::fcntl(fd.as_raw_fd(), libc::F_GETFL);
        libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags & !libc::O_NONBLOCK);
    }
    let socket = File::from(fd);
    let mut reader = socket.try_clone()?;
    let mut writer = socket.try_clone()?;
    let (out, queue) = std_mpsc::channel::<Message>();
    let serial = SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    std::thread::spawn(move || {
        while let Ok(message) = queue.recv() {
            if writer.write_all(&encode(&message)).is_err() {
                break;
            }
        }
    });

    std::thread::spawn(move || {
        let mut codec = Codec::new();
        let mut buffer = [0u8; 16 * 1024];
        let reason = 'read: loop {
            match reader.read(&mut buffer) {
                Ok(0) => break "The phone closed the connection.".to_string(),
                Err(e) => break e.to_string(),
                Ok(n) => codec.feed(&buffer[..n]),
            }
            loop {
                match codec.next() {
                    Ok(Some(message)) => {
                        if tx.send(Event::Peer { serial, message }).is_err() {
                            return;
                        }
                    }
                    Ok(None) => break,
                    // A newer phone app may say things this build doesn't
                    // know; skipping them keeps the rest working.
                    Err(DecodeError::Unknown(_)) => continue,
                    Err(e @ DecodeError::TooLarge(_)) => break 'read e.to_string(),
                }
            }
        };
        let _ = tx.send(Event::LinkDown { serial, reason });
    });

    Ok(Link { device, serial, out, socket })
}

pub async fn register(conn: &Connection, tx: UnboundedSender<Event>) -> zbus::Result<()> {
    conn.object_server().at(PROFILE_PATH, Profile { tx }).await?;
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("Name", Value::from("Omalink"));
    options.insert("Role", Value::from("client"));
    options.insert("RequireAuthentication", Value::from(true));
    options.insert("AutoConnect", Value::from(false));
    conn.call_method(
        Some("org.bluez"),
        "/org/bluez",
        Some("org.bluez.ProfileManager1"),
        "RegisterProfile",
        &(ObjectPath::try_from(PROFILE_PATH)?, SERVICE_UUID, options),
    )
    .await?;
    Ok(())
}
