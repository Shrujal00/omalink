//! BlueZ over D-Bus: the adapter, the devices around it, and pairing.
//!
//! Rather than track every property change, any signal from BlueZ triggers a
//! fresh read of the whole object tree. It is a few kilobytes and keeps this
//! module free of state that could drift from the daemon's.

use std::collections::HashMap;
use std::time::Duration;

use futures_util::StreamExt;
use serde::Serialize;
use tokio::sync::{mpsc::UnboundedSender, oneshot};
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};
use zbus::Connection;

use crate::core::Event;

pub const HFP_AG_UUID: &str = "0000111f-0000-1000-8000-00805f9b34fb";
const AGENT_PATH: &str = "/omalink/agent";

pub type Props = HashMap<String, OwnedValue>;
type Objects = HashMap<OwnedObjectPath, HashMap<String, Props>>;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Adapter {
    pub path: String,
    pub name: String,
    pub powered: bool,
    pub discovering: bool,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Device {
    pub path: String,
    pub address: String,
    pub name: String,
    pub icon: String,
    pub paired: bool,
    pub connected: bool,
    pub trusted: bool,
    /// Signal strength while scanning; 0 when BlueZ has none.
    pub rssi: i16,
    pub phone: bool,
    pub calls: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub adapter: Option<Adapter>,
    pub devices: Vec<Device>,
}

pub fn text(props: &Props, key: &str) -> String {
    props
        .get(key)
        .and_then(|v| v.downcast_ref::<&str>().ok())
        .map(str::to_string)
        .unwrap_or_default()
}

pub fn flag(props: &Props, key: &str) -> bool {
    props.get(key).and_then(|v| v.downcast_ref::<bool>().ok()).unwrap_or(false)
}

fn strings(props: &Props, key: &str) -> Vec<String> {
    let Some(value) = props.get(key) else { return Vec::new() };
    let Ok(array) = value.downcast_ref::<zbus::zvariant::Array>() else { return Vec::new() };
    array
        .iter()
        .filter_map(|v| v.downcast_ref::<&str>().ok().map(str::to_string))
        .collect()
}

pub async fn snapshot(conn: &Connection) -> zbus::Result<Snapshot> {
    let reply = conn
        .call_method(Some("org.bluez"), "/", Some("org.freedesktop.DBus.ObjectManager"), "GetManagedObjects", &())
        .await?;
    let objects: Objects = reply.body().deserialize()?;

    let mut snapshot = Snapshot::default();
    for (path, interfaces) in &objects {
        if let Some(props) = interfaces.get("org.bluez.Adapter1") {
            // The first adapter by path is the one BlueZ calls the default.
            let adapter = Adapter {
                path: path.to_string(),
                name: text(props, "Alias"),
                powered: flag(props, "Powered"),
                discovering: flag(props, "Discovering"),
            };
            if snapshot.adapter.as_ref().is_none_or(|a| adapter.path < a.path) {
                snapshot.adapter = Some(adapter);
            }
        }
        if let Some(props) = interfaces.get("org.bluez.Device1") {
            let uuids = strings(props, "UUIDs");
            let icon = text(props, "Icon");
            let name = text(props, "Alias");
            let address = text(props, "Address");
            snapshot.devices.push(Device {
                path: path.to_string(),
                // Unnamed devices show their address as the alias, with
                // dashes; those are noise in a list of things to pair with.
                name: if name.replace('-', ":") == address { String::new() } else { name },
                address,
                phone: icon == "phone",
                calls: uuids.iter().any(|u| u == HFP_AG_UUID),
                icon,
                paired: flag(props, "Paired"),
                connected: flag(props, "Connected"),
                trusted: flag(props, "Trusted"),
                rssi: props.get("RSSI").and_then(|v| v.downcast_ref::<i16>().ok()).unwrap_or(0),
            });
        }
    }
    if let Some(adapter) = &snapshot.adapter {
        let prefix = format!("{}/", adapter.path);
        snapshot.devices.retain(|d| d.path.starts_with(&prefix));
    }
    // Paired first, then phones, then the strongest signal, so the device the
    // user is looking for is near the top and the list doesn't jump around.
    snapshot.devices.sort_by(|a, b| {
        (b.paired, b.phone, !b.name.is_empty(), b.rssi != 0, b.rssi, &a.address)
            .cmp(&(a.paired, a.phone, !a.name.is_empty(), a.rssi != 0, a.rssi, &b.address))
    });
    Ok(snapshot)
}

/// Sends a snapshot now and after every change BlueZ announces, and again
/// whenever the daemon itself comes or goes.
pub async fn watch(conn: Connection, tx: UnboundedSender<Event>) {
    let rule = |sender: &str| {
        zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(sender.to_string())
            .map(|b| b.build())
    };
    let (Ok(bluez), Ok(bus)) = (rule("org.bluez"), rule("org.freedesktop.DBus")) else { return };
    let Ok(mut changes) = zbus::MessageStream::for_match_rule(bluez, &conn, Some(256)).await else { return };
    let Ok(mut owners) = zbus::MessageStream::for_match_rule(bus, &conn, Some(64)).await else { return };

    loop {
        let snapshot = snapshot(&conn).await.unwrap_or_default();
        if tx.send(Event::Bluez(snapshot)).is_err() {
            return;
        }
        tokio::select! {
            m = changes.next() => if m.is_none() { return },
            m = owners.next() => if m.is_none() { return },
        }
        // A scan announces dozens of devices at once; let the burst settle.
        tokio::time::sleep(Duration::from_millis(200)).await;
        while let Ok(Some(_)) = tokio::time::timeout(Duration::ZERO, changes.next()).await {}
        while let Ok(Some(_)) = tokio::time::timeout(Duration::ZERO, owners.next()).await {}
    }
}

async fn call<B: serde::Serialize + zbus::zvariant::DynamicType>(
    conn: &Connection,
    path: &str,
    interface: &str,
    method: &str,
    body: &B,
) -> Result<(), String> {
    conn.call_method(Some("org.bluez"), path, Some(interface), method, body)
        .await
        .map(|_| ())
        .map_err(explain)
}

/// The computer still holds a pairing the phone has thrown away. Retrying
/// can never work; only pairing afresh does.
pub const STALE_PAIRING: &str = "The phone no longer has this pairing. Pair again, and if the phone still lists this computer under Bluetooth, forget it there first.";

/// BlueZ's errors are terse and some are cryptic; say what the user can do.
pub fn explain(error: zbus::Error) -> String {
    let raw = match &error {
        zbus::Error::MethodError(name, detail, _) => {
            format!("{} {}", name.as_str(), detail.clone().unwrap_or_default())
        }
        other => other.to_string(),
    };
    let lower = raw.to_lowercase();
    if lower.contains("key-missing") {
        STALE_PAIRING.into()
    } else if lower.contains("profile-unavailable") || lower.contains("connection refused") || lower.contains("notavailable") {
        "The phone isn't listening. Open Omalink on the phone.".into()
    } else if lower.contains("page-timeout") || lower.contains("host is down") || lower.contains("connection-unknown") {
        "The phone is out of range or has Bluetooth off.".into()
    } else if lower.contains("authenticationcanceled") || lower.contains("authenticationrejected") {
        "Pairing was declined.".into()
    } else if lower.contains("authenticationfailed") {
        "Pairing failed. Remove this computer from the phone's Bluetooth settings and try again.".into()
    } else if lower.contains("authenticationtimeout") {
        "The phone didn't answer the pairing request in time.".into()
    } else if lower.contains("alreadyexists") {
        "Already paired.".into()
    } else if lower.contains("notready") {
        "Bluetooth is turned off.".into()
    } else if lower.contains("inprogress") {
        "Still working on the last request.".into()
    } else if lower.contains("org.freedesktop.dbus.error.serviceunknown") {
        "The Bluetooth service isn't running.".into()
    } else {
        raw
    }
}

pub async fn set_powered(conn: &Connection, adapter: &str, on: bool) -> Result<(), String> {
    call(conn, adapter, "org.freedesktop.DBus.Properties", "Set", &("org.bluez.Adapter1", "Powered", Value::from(on))).await
}

pub async fn set_discovering(conn: &Connection, adapter: &str, on: bool) -> Result<(), String> {
    call(conn, adapter, "org.bluez.Adapter1", if on { "StartDiscovery" } else { "StopDiscovery" }, &()).await
}

/// Pairs, then trusts, so the phone may reconnect without asking each time.
pub async fn pair(conn: &Connection, device: &str) -> Result<(), String> {
    match call(conn, device, "org.bluez.Device1", "Pair", &()).await {
        Ok(()) => {}
        Err(e) if e == "Already paired." => {}
        Err(e) => return Err(e),
    }
    call(conn, device, "org.freedesktop.DBus.Properties", "Set", &("org.bluez.Device1", "Trusted", Value::from(true))).await
}

pub async fn connect_profile(conn: &Connection, device: &str, uuid: &str) -> Result<(), String> {
    call(conn, device, "org.bluez.Device1", "ConnectProfile", &(uuid,)).await
}

pub async fn disconnect(conn: &Connection, device: &str) -> Result<(), String> {
    call(conn, device, "org.bluez.Device1", "Disconnect", &()).await
}

pub async fn forget(conn: &Connection, adapter: &str, device: &str) -> Result<(), String> {
    let Ok(path) = ObjectPath::try_from(device) else { return Err("Not a device.".into()) };
    call(conn, adapter, "org.bluez.Adapter1", "RemoveDevice", &(path,)).await
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
enum AgentError {
    #[zbus(error)]
    ZBus(zbus::Error),
    Rejected(String),
}

/// Answers BlueZ's pairing questions by asking the window.
struct Agent {
    tx: UnboundedSender<Event>,
}

impl Agent {
    async fn ask(&self, device: &ObjectPath<'_>, code: String, confirm: bool) -> Result<(), AgentError> {
        let (reply, answer) = oneshot::channel();
        let request = Event::PairRequest { device: device.to_string(), code, confirm, reply: Some(reply) };
        if self.tx.send(request).is_err() {
            return Err(AgentError::Rejected("closing".into()));
        }
        match tokio::time::timeout(Duration::from_secs(60), answer).await {
            Ok(Ok(true)) => Ok(()),
            _ => Err(AgentError::Rejected("declined".into())),
        }
    }
}

#[zbus::interface(name = "org.bluez.Agent1")]
impl Agent {
    async fn release(&self) {}

    async fn request_confirmation(&self, device: ObjectPath<'_>, passkey: u32) -> Result<(), AgentError> {
        self.ask(&device, format!("{passkey:06}"), true).await
    }

    async fn request_authorization(&self, device: ObjectPath<'_>) -> Result<(), AgentError> {
        self.ask(&device, String::new(), true).await
    }

    /// Only devices the user paired from this window reach here.
    async fn authorize_service(&self, _device: ObjectPath<'_>, _uuid: String) {}

    async fn display_passkey(&self, device: ObjectPath<'_>, passkey: u32, _entered: u16) {
        let _ = self.tx.send(Event::PairRequest {
            device: device.to_string(),
            code: format!("{passkey:06}"),
            confirm: false,
            reply: None,
        });
    }

    async fn display_pin_code(&self, device: ObjectPath<'_>, pincode: String) {
        let _ = self.tx.send(Event::PairRequest { device: device.to_string(), code: pincode, confirm: false, reply: None });
    }

    async fn request_pin_code(&self, _device: ObjectPath<'_>) -> Result<String, AgentError> {
        // Phones use numeric comparison; a device that wants a typed PIN is
        // not one this app pairs with.
        Err(AgentError::Rejected("PIN entry is not supported".into()))
    }

    async fn request_passkey(&self, _device: ObjectPath<'_>) -> Result<u32, AgentError> {
        Err(AgentError::Rejected("passkey entry is not supported".into()))
    }

    async fn cancel(&self) {
        let _ = self.tx.send(Event::PairCancelled);
    }
}

/// Registers the agent for pairings this process starts. It is not made the
/// system default, so pairing from the desktop's own Bluetooth UI is untouched.
pub async fn register_agent(conn: &Connection, tx: UnboundedSender<Event>) -> zbus::Result<()> {
    conn.object_server().at(AGENT_PATH, Agent { tx }).await?;
    let path = ObjectPath::try_from(AGENT_PATH)?;
    conn.call_method(
        Some("org.bluez"),
        "/org/bluez",
        Some("org.bluez.AgentManager1"),
        "RegisterAgent",
        &(path, "KeyboardDisplay"),
    )
    .await?;
    Ok(())
}
