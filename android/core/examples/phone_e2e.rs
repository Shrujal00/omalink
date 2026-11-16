//! Runs the phone app's network core against a desktop started from a debug
//! build, which logs the icon it shows. Pairs (tapping the icon the desktop
//! logged, found among the three offered), then sends a file and keeps any
//! that arrive.
//!
//! ```sh
//! cargo run -p omalink-core --example phone_e2e -- STATE_DIR DESKTOP_LOG [FILE_TO_SEND]
//! ```

use std::path::PathBuf;
use std::time::{Duration, Instant};

use omalink_core::net::Phone;
use serde_json::Value;

/// Waits for the desktop to log the icon it shows.
fn shown_icon(log: &str, after: usize) -> Option<String> {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        let text = std::fs::read_to_string(log).unwrap_or_default();
        if let Some(line) = text.lines().skip(after).filter(|l| l.contains("showing icon ")).last() {
            return line.split("showing icon ").nth(1).map(str::to_string);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    None
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, log) = (PathBuf::from(&args[0]), args[1].clone());
    let send = args.get(2).map(PathBuf::from);
    std::fs::create_dir_all(&dir).unwrap();
    let phone = Phone::start(&dir, "E2E phone").expect("start");
    println!("phone {}", phone.id());
    let paired_already = phone.peers() != "[]";
    let mut log_lines = std::fs::read_to_string(&log).unwrap_or_default().lines().count();
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut sent = false;
    let mut received = 0;
    let mut pairing = false;

    while Instant::now() < deadline {
        let Some(event) = phone.next_event(Duration::from_millis(500)) else { continue };
        let event: Value = serde_json::from_str(&event).unwrap();
        println!("event {}", event.to_string().chars().take(160).collect::<String>());
        match event["ev"].as_str().unwrap_or_default() {
            "found" if !paired_already && !pairing => {
                pairing = true;
                phone.pair(event["id"].as_str().unwrap(), event["name"].as_str().unwrap());
            }
            "verify" => {
                let name = shown_icon(&log, log_lines).expect("the desktop never showed an icon");
                log_lines = std::fs::read_to_string(&log).unwrap_or_default().lines().count();
                let index = event["choices"].as_array().unwrap().iter().position(|c| c["name"] == name.as_str());
                println!("desktop shows {name}; offered at {index:?}");
                phone.choose(index.expect("the desktop's icon is not among the phone's three"));
            }
            "verify_done" => println!("verification {} -> {}", event["purpose"], event["ok"]),
            "link" if event["connected"] == true && !sent => {
                sent = true;
                if let Some(path) = &send {
                    let bytes = std::fs::read(path).unwrap();
                    let name = path.file_name().unwrap().to_string_lossy();
                    let handle = phone.open_file(77, &name, bytes.len() as i64);
                    assert!(handle > 0, "no stream");
                    for chunk in bytes.chunks(64 * 1024) {
                        assert!(phone.write(handle, chunk));
                    }
                    assert!(phone.finish(handle));
                    println!("sent {name} ({} bytes)", bytes.len());
                }
            }
            "file" => {
                let handle = event["handle"].as_i64().unwrap();
                let mut out = Vec::new();
                let mut buffer = vec![0u8; 64 * 1024];
                loop {
                    let n = phone.read(handle, &mut buffer);
                    assert!(n >= 0, "read failed");
                    if n == 0 {
                        break;
                    }
                    out.extend_from_slice(&buffer[..n as usize]);
                }
                let name = event["name"].as_str().unwrap();
                std::fs::write(dir.join(name), &out).unwrap();
                let saved = serde_json::json!({ "t": "file_saved", "d": { "id": event["id"], "ok": true, "error": "" } });
                assert!(phone.send(&saved.to_string()));
                println!("received {name} ({} bytes)", out.len());
                received += 1;
            }
            "message" => {
                let message: Value = serde_json::from_str(event["json"].as_str().unwrap()).unwrap();
                if message["t"] == "sync" {
                    phone.send(r#"{"t":"status","d":{"battery":71,"charging":true,"signal":4}}"#);
                }
            }
            _ => {}
        }
        if sent && received > 0 {
            break;
        }
    }
    println!("done: sent={sent} received={received}");
}
