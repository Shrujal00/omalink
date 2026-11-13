//! Calls over Bluetooth hands-free, through PipeWire.
//!
//! WirePlumber acts as the hands-free unit and publishes an oFono-style API
//! at `org.pipewire.Telephony`. Each connected phone is a "modem". PipeWire
//! also carries the call audio to the default speakers and microphone, so
//! this module only has to place, answer, and end calls.

use std::collections::HashMap;
use std::time::Duration;

use futures_util::StreamExt;
use tokio::sync::mpsc::UnboundedSender;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::Connection;

use crate::bluez::text;
use crate::core::Event;

const SERVICE: &str = "org.pipewire.Telephony";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Call {
    pub path: String,
    /// oFono's words: incoming, waiting, dialing, alerting, active, held.
    pub state: String,
    pub number: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    /// The phone's hands-free connection, when there is one.
    pub modem: Option<String>,
    pub calls: Vec<Call>,
}

type Listing = Vec<(OwnedObjectPath, HashMap<String, OwnedValue>)>;

async fn read(conn: &Connection) -> State {
    let mut state = State::default();
    let Ok(reply) = conn
        .call_method(Some(SERVICE), "/org/pipewire/Telephony", Some("org.ofono.Manager"), "GetModems", &())
        .await
    else {
        return state;
    };
    let Ok(modems) = reply.body().deserialize::<Listing>() else { return state };
    let Some((modem, _)) = modems.into_iter().next() else { return state };
    state.modem = Some(modem.to_string());

    if let Ok(reply) = conn
        .call_method(Some(SERVICE), modem.as_str(), Some("org.ofono.VoiceCallManager"), "GetCalls", &())
        .await
    {
        if let Ok(calls) = reply.body().deserialize::<Listing>() {
            state.calls = calls
                .into_iter()
                .map(|(path, props)| Call {
                    path: path.to_string(),
                    state: text(&props, "State"),
                    number: text(&props, "LineIdentification"),
                })
                .filter(|c| c.state != "disconnected")
                .collect();
        }
    }
    state
}

pub async fn watch(conn: Connection, tx: UnboundedSender<Event>) {
    let rule = |sender: &str| {
        zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(sender.to_string())
            .map(|b| b.build())
    };
    let (Ok(service), Ok(bus)) = (rule(SERVICE), rule("org.freedesktop.DBus")) else { return };
    let Ok(mut changes) = zbus::MessageStream::for_match_rule(service, &conn, Some(64)).await else { return };
    let Ok(mut owners) = zbus::MessageStream::for_match_rule(bus, &conn, Some(64)).await else { return };

    let mut last = None;
    loop {
        let state = read(&conn).await;
        if last.as_ref() != Some(&state) {
            last = Some(state.clone());
            if tx.send(Event::Telephony(state)).is_err() {
                return;
            }
        }
        // The poll is a safety net: a missed signal must not leave a call
        // showing on screen after it ended.
        tokio::select! {
            m = changes.next() => if m.is_none() { return },
            m = owners.next() => if m.is_none() { return },
            _ = tokio::time::sleep(Duration::from_secs(if last.as_ref().is_some_and(|s| !s.calls.is_empty()) { 2 } else { 15 })) => {}
        }
        tokio::time::sleep(Duration::from_millis(80)).await;
    }
}

async fn call<B: serde::Serialize + zbus::zvariant::DynamicType>(
    conn: &Connection,
    path: &str,
    interface: &str,
    method: &str,
    body: &B,
) -> Result<(), String> {
    conn.call_method(Some(SERVICE), path, Some(interface), method, body)
        .await
        .map(|_| ())
        .map_err(|e| match e {
            zbus::Error::MethodError(_, Some(detail), _) if !detail.is_empty() => detail,
            other => other.to_string(),
        })
}

pub async fn dial(conn: &Connection, modem: &str, number: &str) -> Result<(), String> {
    call(conn, modem, "org.ofono.VoiceCallManager", "Dial", &(number, "default")).await
}

pub async fn answer(conn: &Connection, call_path: &str) -> Result<(), String> {
    call(conn, call_path, "org.ofono.VoiceCall", "Answer", &()).await
}

pub async fn hangup(conn: &Connection, call_path: &str) -> Result<(), String> {
    call(conn, call_path, "org.ofono.VoiceCall", "Hangup", &()).await
}

pub async fn tones(conn: &Connection, modem: &str, tones: &str) -> Result<(), String> {
    call(conn, modem, "org.ofono.VoiceCallManager", "SendTones", &(tones,)).await
}
