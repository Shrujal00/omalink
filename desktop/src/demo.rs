//! `omalink --demo`: the window filled with made-up data and no Bluetooth,
//! for trying the interface and for screenshots.

use omalink_proto::{CallEntry, CallKind, Contact, Conversation, Notification, PhoneStatus, Sms};
use serde_json::{json, Value as Json};

use crate::core::{now_ms, Clip, Core, Transfer};

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
    core.granted = ["bluetooth", "post_notifications", "notifications", "sms", "contacts", "call_log", "phone", "photos"].map(String::from).to_vec();
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
    let net = |pairing: Json| {
        json!({
            "state": "ready", "error": "", "id": "demo", "name": "omarchy", "connected": true, "route": "direct", "relay": "",
            "phones": [{ "id": "demo-phone", "name": "Pixel 9", "added": now_ms() - 86_400_000, "connected": true }],
            "pairing": pairing,
        })
    };
    core.send_ui("net", net(Json::Null));
    seed_files(core);
    let mirror = |state: &str, icon: &str, name: &str| {
        json!({ "state": state, "icon": icon, "icon_name": name, "width": 0, "height": 0, "control": true, "fps": 0, "reason": "", "available": true, "route": "direct" })
    };
    core.send_ui("mirror", mirror("idle", "", ""));

    // For screenshots: OMALINK_SHOT=/path/to.png[:page[:call[:delay_ms]]] renders one
    // page to a file and exits, without needing a display.
    if let Ok(shot) = std::env::var("OMALINK_SHOT") {
        let mut parts = shot.split(':');
        let path = parts.next().unwrap_or_default();
        // A page by number or by name ("home", "files", …).
        let page = match parts.next() {
            Some(p) => p.parse::<i64>().map(Json::from).unwrap_or_else(|_| Json::from(p)),
            None => Json::from(0),
        };
        match parts.next() {
            Some("incoming") => core.send_ui("call", json!({ "phase": "incoming", "number": "+1 555 010 4477", "name": "Maya Lindqvist", "since": 0, "audio": true, "muted": false })),
            Some("active") => core.send_ui("call", json!({ "phase": "active", "number": "+1 555 010 4477", "name": "Maya Lindqvist", "since": now_ms() - 83_000, "audio": true, "muted": false })),
            Some("drop") => core.send_ui("demo_drop", json!(true)),
            Some("gallery") => core.send_ui("demo_tab", json!("phone")),
            Some("mirror-verify") => core.send_ui("mirror", mirror("verify", "🐙", "octopus")),
            Some("pairing") => core.send_ui("net", net(json!({ "phone": "Pixel 9", "icon": "🦊", "icon_name": "fox", "attempts": 0, "left": 3 }))),
            _ => {}
        }
        let delay = parts.next().and_then(|p| p.parse::<i64>().ok()).unwrap_or(900);
        // OMALINK_SHOT_SIZE=1900x1100 renders at that window size.
        let (width, height) = std::env::var("OMALINK_SHOT_SIZE")
            .ok()
            .and_then(|s| s.split_once('x').and_then(|(w, h)| Some((w.parse::<i64>().ok()?, h.parse::<i64>().ok()?))))
            .unwrap_or((0, 0));
        core.send_ui("shot", json!({ "path": path, "page": page, "delay": delay, "width": width, "height": height }));
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

/// Files for the Files page: Omarchy's own theme pictures and intro videos
/// where they exist, so the previews are real, and a few documents.
fn seed_files(core: &mut Core) {
    let themes = std::path::Path::new("/usr/share/omarchy/themes");
    let mut media: Vec<std::path::PathBuf> = ["catppuccin/backgrounds/1-totoro.webp", "ethereal/backgrounds/2-meadow.webp", "catppuccin/backgrounds/intros/2-waves.mp4",
        "catppuccin/backgrounds/3-blue-eye.webp", "everforest/backgrounds/intros/1-tree-tops.mp4", "ethereal/backgrounds/1-cosmic.webp"]
        .iter()
        .map(|p| themes.join(p))
        .collect();
    media.retain(|p| p.exists());
    let now = now_ms();
    let mut files: Vec<Transfer> = Vec::new();
    let mut add = |name: String, path: String, size: i64, incoming: bool, state: &'static str, minutes: i64| {
        let key = format!("demo{}", files.len());
        files.push(Transfer {
            key,
            kind: crate::thumbs::kind(&name),
            name,
            size,
            done: if state == "active" { (size as f64 * 0.42) as u64 } else { size.max(0) as u64 },
            incoming,
            state,
            path,
            time: now - minutes * 60_000,
            via: "network",
            speed: if state == "active" { 3_400_000.0 } else { 0.0 },
            eta: if state == "active" { 7 } else { -1 },
            ..Default::default()
        });
    };
    for (i, path) in media.iter().enumerate() {
        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let size = std::fs::metadata(path).map(|m| m.len() as i64).unwrap_or(0);
        add(name, path.to_string_lossy().into_owned(), size, i % 3 != 1, if i == 0 { "active" } else { "done" }, i as i64 * 7);
    }
    add("Lease agreement.pdf".into(), "/nonexistent/Lease agreement.pdf".into(), 1_284_000, true, "done", 55);
    add("omalink.apk".into(), "/nonexistent/omalink.apk".into(), 11_201_439, false, "done", 80);
    add("Holiday plans.zip".into(), "/nonexistent/Holiday plans.zip".into(), 48_000_000, false, "failed", 95);
    add("Voice note.m4a".into(), "/nonexistent/Voice note.m4a".into(), 820_000, true, "done", 130);
    if let Some(failed) = files.iter_mut().find(|t| t.state == "failed") {
        failed.error = "The connection to the phone was lost.".into();
    }
    core.transfers = files;
    // A wallpaper for the phone, from the same place.
    let wallpaper = themes.join("ethereal/backgrounds/1-cosmic.webp");
    if wallpaper.exists() {
        core.send_ui("wallpaper", json!(format!("file://{}", wallpaper.display())));
    }
    core.send_ui("transfers", json!(core.transfers));
    // A gallery made of the same pictures, a few days' worth.
    let gallery: Vec<Json> = (0..18)
        .filter_map(|i| {
            let path = media.get(i % media.len().max(1))?;
            let video = path.extension().is_some_and(|e| e == "mp4");
            let thumb = if video { String::new() } else { format!("file://{}", path.display()) };
            Some(json!({ "id": i, "name": format!("IMG_{i:04}"), "kind": if video { "video" } else { "image" },
                         "time": now - (i as i64 / 6) * 86_400_000 - i as i64 * 600_000, "size": 2_400_000,
                         "width": 1080, "height": 1920, "duration_ms": if video { 14_000 } else { 0 }, "thumb": thumb }))
        })
        .collect();
    core.send_ui("gallery", json!({ "items": gallery, "more": false, "loading": false }));
    core.make_thumbs();
}
