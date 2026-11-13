//! `omalink --demo`: the window filled with made-up data and no Bluetooth,
//! for trying the interface and for screenshots.

use omalink_proto::{CallEntry, CallKind, Contact, Conversation, Notification, PhoneStatus, Sms};
use serde_json::{json, Value as Json};

use crate::core::{now_ms, Clip, Core};

const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;

fn thread(id: i64) -> Vec<Sms> {
    let now = now_ms();
    let lines: &[(bool, &str, i64)] = match id {
        1 => &[
            (false, "Are we still on for the ridge trail Saturday?", 26 * HOUR),
            (true, "Yes! Forecast says clear until late afternoon.", 26 * HOUR - 4 * MINUTE),
            (false, "Perfect. I'll bring the stove, you bring coffee?", 25 * HOUR),
            (true, "Deal. Leaving at 7 so we beat the crowd.", 25 * HOUR - 2 * MINUTE),
            (false, "7 it is. Can you send me the trailhead pin?", 9 * MINUTE),
            (true, "Sending it from my desk now.", 7 * MINUTE),
            (false, "Got it, thanks. See you Saturday!", 4 * MINUTE),
        ],
        2 => &[
            (false, "Your table for four is confirmed for Friday at 19:30.", 3 * HOUR),
            (true, "Thank you. Could we get a spot by the window?", 3 * HOUR - 5 * MINUTE),
            (false, "Noted — we'll do our best.", 2 * HOUR),
        ],
        3 => &[
            (true, "Did the parcel arrive?", 30 * HOUR),
            (false, "It did. Left it on your desk.", 29 * HOUR),
        ],
        _ => &[(false, "Your verification code is 482 913. It expires in 10 minutes.", 50 * HOUR)],
    };
    lines
        .iter()
        .enumerate()
        .map(|(i, (outgoing, body, ago))| Sms {
            id: id * 100 + i as i64,
            thread_id: id,
            address: String::new(),
            body: body.to_string(),
            time: now - ago,
            outgoing: *outgoing,
            read: true,
        })
        .collect()
}

pub fn seed(core: &mut Core) {
    let now = now_ms();
    core.status = Some(PhoneStatus { battery: 78, charging: false, signal: 3, carrier: "Northwind".into(), ringer: "normal".into() });
    core.granted = ["bluetooth", "post_notifications", "notifications", "sms", "contacts", "call_log", "phone"].map(String::from).to_vec();
    core.missing = vec!["battery".into()];
    let conversation = |thread_id, name: &str, address: &str, snippet: &str, ago, unread| Conversation {
        thread_id,
        address: address.into(),
        name: name.into(),
        snippet: snippet.into(),
        time: now - ago,
        unread,
    };
    core.conversations = vec![
        conversation(1, "Maya Lindqvist", "+1 555 010 4477", "Got it, thanks. See you Saturday!", 4 * MINUTE, 1),
        conversation(2, "Harbor Kitchen", "+1 555 010 2030", "Noted — we'll do our best.", 2 * HOUR, 0),
        conversation(3, "Tomás Ferreira", "+1 555 010 8812", "It did. Left it on your desk.", 29 * HOUR, 0),
        conversation(4, "", "48291", "Your verification code is 482 913. It expires in 10 minutes.", 50 * HOUR, 0),
    ];
    let call = |id, name: &str, number: &str, kind, ago, duration| CallEntry {
        id,
        number: number.into(),
        name: name.into(),
        kind,
        time: now - ago,
        duration,
    };
    core.call_log = vec![
        call(1, "Maya Lindqvist", "+1 555 010 4477", CallKind::Incoming, 38 * MINUTE, 312),
        call(2, "Tomás Ferreira", "+1 555 010 8812", CallKind::Missed, 3 * HOUR, 0),
        call(3, "Harbor Kitchen", "+1 555 010 2030", CallKind::Outgoing, 5 * HOUR, 96),
        call(4, "", "+1 555 010 6621", CallKind::Missed, 27 * HOUR, 0),
        call(5, "Ingrid Sørensen", "+1 555 010 3349", CallKind::Outgoing, 30 * HOUR, 1284),
        call(6, "Maya Lindqvist", "+1 555 010 4477", CallKind::Incoming, 52 * HOUR, 45),
    ];
    core.contacts = [
        ("Harbor Kitchen", "+1 555 010 2030"),
        ("Ingrid Sørensen", "+1 555 010 3349"),
        ("Maya Lindqvist", "+1 555 010 4477"),
        ("Tomás Ferreira", "+1 555 010 8812"),
    ]
    .map(|(name, number)| Contact { name: name.into(), numbers: vec![number.into()], photo: 0 })
    .to_vec();
    let notification = |key: &str, app: &str, title: &str, text: &str, ago, actions: &[&str], can_reply| Notification {
        key: key.into(),
        package: format!("demo.{}", app.to_lowercase()),
        app: app.into(),
        title: title.into(),
        text: text.into(),
        time: now - ago,
        actions: actions.iter().map(|a| a.to_string()).collect(),
        can_reply,
        clearable: true,
        icon: String::new(),
    };
    core.notifications = vec![
        notification("a", "Messages", "Maya Lindqvist", "Got it, thanks. See you Saturday!", 4 * MINUTE, &["Mark as read"], true),
        notification("b", "Calendar", "Design review", "Starts in 20 minutes · Room 3", 11 * MINUTE, &["Snooze"], false),
        notification("c", "Mail", "Ingrid Sørensen", "Re: Q3 numbers — looks good to me, ship it.", 47 * MINUTE, &["Archive"], true),
        notification("d", "Weather", "Rain expected at 17:00", "Light showers for about an hour.", 2 * HOUR, &[], false),
    ];
    core.clips = vec![
        Clip { text: "https://example.com/trailhead-map".into(), time: now - 7 * MINUTE, from: "desktop" },
        Clip { text: "482 913".into(), time: now - 50 * HOUR, from: "phone" },
    ];

    core.send_ui("adapter", json!({ "path": "/demo", "name": "desk", "powered": true, "discovering": false }));
    core.send_ui(
        "devices",
        json!([
            { "path": "/demo/phone", "address": "00:00:00:00:00:01", "name": "Pixel 9", "icon": "phone", "paired": true, "connected": true, "trusted": true, "rssi": 0, "phone": true, "calls": true },
            { "path": "/demo/buds", "address": "00:00:00:00:00:02", "name": "Studio Buds", "icon": "audio-headset", "paired": false, "connected": false, "trusted": false, "rssi": -58, "phone": false, "calls": false },
        ]),
    );
    core.send_ui("settings", json!({ "device": "00:00:00:00:00:01", "clipboard": true, "toasts": true, "calls": true }));
    core.send_ui("link", json!({ "state": "connected", "address": "00:00:00:00:00:01", "name": "Pixel 9", "error": "", "calls": true }));
    core.send_ui("call", json!({ "phase": "idle", "number": "", "name": "", "since": 0, "audio": false, "muted": false }));
    core.emit_phone();

    // For screenshots: OMALINK_SHOT=/path/to.png[:page[:call[:delay_ms]]] renders one
    // page to a file and exits, without needing a display.
    if let Ok(shot) = std::env::var("OMALINK_SHOT") {
        let mut parts = shot.split(':');
        let path = parts.next().unwrap_or_default();
        let page = parts.next().and_then(|p| p.parse::<i64>().ok()).unwrap_or(0);
        match parts.next() {
            Some("incoming") => core.send_ui("call", json!({ "phase": "incoming", "number": "+1 555 010 4477", "name": "Maya Lindqvist", "since": 0, "audio": true, "muted": false })),
            Some("active") => core.send_ui("call", json!({ "phase": "active", "number": "+1 555 010 4477", "name": "Maya Lindqvist", "since": now_ms() - 83_000, "audio": true, "muted": false })),
            _ => {}
        }
        let delay = parts.next().and_then(|p| p.parse::<i64>().ok()).unwrap_or(900);
        core.send_ui("shot", json!({ "path": path, "page": page, "delay": delay }));
    }
}

/// Answers the few commands that make the demo feel alive. Returns whether
/// the command was handled; nothing in demo mode reaches a real device.
pub fn handle(core: &mut Core, name: &str, args: &Json) -> bool {
    match name {
        "open_thread" => {
            let id = args.get("thread_id").and_then(Json::as_i64).unwrap_or(0);
            core.send_ui("messages", json!({ "thread_id": id, "items": thread(id) }));
            if let Some(c) = core.conversations.iter_mut().find(|c| c.thread_id == id) {
                c.unread = 0;
            }
            core.send_ui("conversations", json!(core.conversations));
        }
        "dismiss" => {
            let key = args.get("key").and_then(Json::as_str).unwrap_or_default().to_string();
            core.notifications.retain(|n| n.key != key);
            core.emit_phone();
        }
        "dismiss_all" => {
            core.notifications.clear();
            core.emit_phone();
        }
        "dial" | "demo_incoming" => {
            let number = args.get("number").and_then(Json::as_str).unwrap_or("+1 555 010 4477");
            let name = core.contacts.iter().find(|c| c.numbers.iter().any(|n| n == number)).map(|c| c.name.clone()).unwrap_or_default();
            let phase = if name == "demo_incoming" { "incoming" } else { "dialing" };
            core.send_ui("call", json!({ "phase": phase, "number": number, "name": name, "since": 0, "audio": true, "muted": false }));
        }
        "answer" => {
            core.send_ui("call", json!({ "phase": "active", "number": "+1 555 010 4477", "name": "Maya Lindqvist", "since": now_ms(), "audio": true, "muted": false }));
        }
        "hangup" => {
            core.send_ui("call", json!({ "phase": "idle", "number": "", "name": "", "since": 0, "audio": false, "muted": false }));
        }
        _ => {}
    }
    true
}
