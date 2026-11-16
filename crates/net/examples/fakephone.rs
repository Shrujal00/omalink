//! A stand-in for the phone app, for trying the desktop without a phone.
//!
//! ```sh
//! # Pair: finds a computer that is pairing on this network (or --id),
//! # prints the three icons and asks which one the desktop shows.
//! cargo run -p omalink-net --example fakephone -- --state /tmp/fake pair
//!
//! # Connect as the paired phone, answer the desktop, send a file, and keep
//! # whatever the desktop sends into /tmp/fake/received.
//! cargo run -p omalink-net --example fakephone -- --state /tmp/fake link --send photo.jpg --seconds 30
//! ```
//!
//! `--auto` taps the right icon by itself, for scripted tests only: it skips
//! the very check the icons exist for.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use n0_future::StreamExt;
use omalink_net::{load_key, open_messages, random32, read_header, receive_file, send_file, Found, Node, Peer, Role, Trust};
use omalink_proto::net::{LINK_ALPN, PAIR_ALPN};
use omalink_proto::net::VideoFrame;
use omalink_proto::pair::{Chooser, ICONS, MIRROR, PAIR};
use omalink_proto::{permission, Message, PhoneStatus, PROTOCOL_VERSION};
use tokio::sync::mpsc::unbounded_channel;

struct Args {
    state: PathBuf,
    command: String,
    id: Option<String>,
    auto: bool,
    wrong: bool,
    send: Vec<PathBuf>,
    seconds: u64,
    video: Option<PathBuf>,
}

fn args() -> Args {
    let mut args = Args { state: PathBuf::from("/tmp/omalink-fakephone"), command: String::new(), id: None, auto: false, wrong: false, send: Vec::new(), seconds: 30, video: None };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--state" => args.state = it.next().map(PathBuf::from).expect("--state DIR"),
            "--id" => args.id = it.next(),
            "--auto" => args.auto = true,
            // Taps a decoy, to see a mismatch refused.
            "--wrong" => args.wrong = true,
            "--send" => args.send.push(it.next().map(PathBuf::from).expect("--send FILE")),
            // An H.264 Annex B file to stream when the desktop asks for the screen.
            "--video" => args.video = it.next().map(PathBuf::from),
            "--seconds" => args.seconds = it.next().and_then(|s| s.parse().ok()).expect("--seconds N"),
            other => args.command = other.to_string(),
        }
    }
    args
}

#[tokio::main]
async fn main() {
    // RUST_LOG=iroh=debug shows how the connection is found.
    let _ = tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).with_writer(std::io::stderr).try_init();
    let args = args();
    let key = load_key(&args.state.join("key")).expect("key");
    let node = Node::start(key, Role::Phone, "").await.expect("node");
    println!("fakephone {}", node.id());
    let result = match args.command.as_str() {
        "pair" => pair(&node, &args).await,
        "link" => link(&node, &args).await,
        _ => Err("usage: fakephone [--state DIR] pair [--id ID] [--auto] | link [--send FILE] [--seconds N]".into()),
    };
    node.close().await;
    if let Err(e) = result {
        eprintln!("fakephone: {e}");
        std::process::exit(1);
    }
}

async fn pair(node: &Node, args: &Args) -> Result<(), String> {
    let (id, name) = match &args.id {
        Some(id) => (id.clone(), String::new()),
        None => {
            println!("looking for a computer that is pairing…");
            let mut found = node.browse().await;
            let wait = tokio::time::timeout(Duration::from_secs(20), async {
                while let Some(event) = found.next().await {
                    if let Found::Open { id, name } = event {
                        return Some((id, name));
                    }
                }
                None
            });
            wait.await.ok().flatten().ok_or("no computer is pairing on this network")?
        }
    };
    println!("pairing with {} ({id})", if name.is_empty() { "computer" } else { &name });
    let conn = node.dial(&id, PAIR_ALPN).await?;
    let desktop = omalink_net::id_bytes(&conn.remote_id());
    let phone = omalink_net::id_bytes(&node.endpoint.id());
    let (mut send, mut reader) = open_messages(&conn, &Message::Hello { version: PROTOCOL_VERSION, name: "Fake phone".into() }).await?;
    let mut chooser = Chooser::new(PAIR, &desktop, &phone, &random32()).map_err(|e| format!("{e:?}"))?;
    omalink_net::write(&mut send, &Message::VerifyCommit { purpose: PAIR.into(), commit: chooser.commitment() }).await?;
    let nonce = match reader.next().await? {
        Some(Message::VerifyNonce { nonce }) => nonce,
        Some(Message::VerifyDone { ok: false }) => return Err("the computer refused".into()),
        other => return Err(format!("unexpected {other:?}")),
    };
    let (reveal, sas) = chooser.nonce(&nonce).map_err(|e| format!("{e:?}"))?;
    omalink_net::write(&mut send, &Message::VerifyReveal { nonce: reveal }).await?;

    for (i, &choice) in sas.choices.iter().enumerate() {
        println!("  {}. {} {}", i + 1, ICONS[choice].0, ICONS[choice].1);
    }
    let tapped = if args.wrong {
        let decoy = *sas.choices.iter().find(|&&c| c != sas.icon).expect("a decoy");
        println!("--wrong: tapping {}", ICONS[decoy].1);
        decoy
    } else if args.auto {
        println!("--auto: tapping {}", ICONS[sas.icon].1);
        sas.icon
    } else {
        print!("which one does the computer show? ");
        std::io::stdout().flush().ok();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).map_err(|e| e.to_string())?;
        let n: usize = line.trim().parse().map_err(|_| "type 1, 2 or 3")?;
        *sas.choices.get(n.wrapping_sub(1)).ok_or("type 1, 2 or 3")?
    };
    let ok = chooser.check(tapped);
    omalink_net::write(&mut send, &Message::VerifyResult { ok }).await?;
    let done = reader.next().await?;
    conn.close(0u32.into(), b"done");
    match done {
        Some(Message::VerifyDone { ok: true }) if ok => {
            let mut trust = Trust::load(args.state.join("peers.json"));
            let addr = node.known_addr(&id).await;
            trust.add(Peer { id, name: if name.is_empty() { "Computer".into() } else { name }, added: 0, addr });
            println!("paired");
            Ok(())
        }
        _ => Err("pairing failed: the icons didn't match".into()),
    }
}

async fn link(node: &Node, args: &Args) -> Result<(), String> {
    let trust = Trust::load(args.state.join("peers.json"));
    let desktop = trust.peers.first().ok_or("not paired: run `pair` first")?.clone();
    println!("connecting to {}…", desktop.name);
    let conn = node.dial_peer(&desktop, LINK_ALPN).await?;
    println!("connected ({})", omalink_net::route(&conn));
    let (mut send, mut reader) = open_messages(&conn, &Message::Hello { version: PROTOCOL_VERSION, name: "Fake phone".into() }).await?;

    let (out, mut queue) = unbounded_channel::<Message>();
    tokio::spawn(async move {
        while let Some(message) = queue.recv().await {
            if omalink_net::write(&mut send, &message).await.is_err() {
                break;
            }
        }
    });

    // Files from the desktop.
    let received = args.state.join("received");
    let streams = conn.clone();
    let replies = out.clone();
    tokio::spawn(async move {
        while let Ok(mut recv) = streams.accept_uni().await {
            if let Ok(Message::FileStart { id, name, size }) = read_header(&mut recv).await {
                let _ = std::fs::create_dir_all(&received);
                let path = received.join(std::path::Path::new(&name).file_name().unwrap_or_default());
                let file = tokio::fs::File::create(&path).await.expect("create");
                let result = receive_file(&mut recv, file, size, |_| {}).await;
                println!("received {} ({:?})", path.display(), result);
                let _ = replies.send(Message::FileSaved { id, ok: result.is_ok(), error: result.err().unwrap_or_default() });
            }
        }
    });

    for (i, path) in args.send.iter().enumerate() {
        let file = tokio::fs::File::open(path).await.map_err(|e| e.to_string())?;
        let size = file.metadata().await.map_err(|e| e.to_string())?.len() as i64;
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let sent = send_file(&conn, 1000 + i as u64, &name, size, file, |_| {}).await?;
        println!("sent {name} ({sent} bytes, {})", omalink_net::route(&conn));
    }

    let mut mirror: Option<Chooser> = None;
    let deadline = tokio::time::sleep(Duration::from_secs(args.seconds));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => break,
            message = reader.next() => {
                let Some(message) = message? else { break };
                println!("< {}", serde_json::to_string(&message).unwrap_or_default().chars().take(160).collect::<String>());
                match message {
                    Message::Ping { n } => { let _ = out.send(Message::Pong { n }); }
                    Message::Sync { .. } => {
                        let _ = out.send(Message::Status(PhoneStatus { battery: 64, charging: false, signal: 3, carrier: "Fake".into(), ringer: "normal".into() }));
                        let _ = out.send(Message::Permissions { granted: permission::ALL.iter().map(|s| s.to_string()).collect(), missing: vec![] });
                    }
                    Message::Clipboard { text } => println!("clipboard: {text}"),
                    Message::MirrorRequest { max_size } => {
                        println!("mirror requested (max {max_size})");
                        let chooser = Chooser::new(MIRROR, &omalink_net::id_bytes(&conn.remote_id()), &omalink_net::id_bytes(&node.endpoint.id()), &random32()).expect("chooser");
                        let _ = out.send(Message::VerifyCommit { purpose: MIRROR.into(), commit: chooser.commitment() });
                        mirror = Some(chooser);
                    }
                    Message::VerifyNonce { nonce } => {
                        if let Some(chooser) = mirror.as_mut() {
                            let (reveal, sas) = chooser.nonce(&nonce).expect("nonce");
                            let _ = out.send(Message::VerifyReveal { nonce: reveal });
                            println!("mirror icons: {}; tapping {}", sas.choices.iter().map(|&c| ICONS[c].1).collect::<Vec<_>>().join(", "), ICONS[sas.icon].1);
                            let _ = out.send(Message::VerifyResult { ok: chooser.check(sas.icon) });
                        }
                    }
                    Message::VerifyDone { ok } => {
                        println!("mirror verified: {ok}");
                        if ok {
                            let _ = out.send(Message::ControlState { available: true });
                            if let Some(path) = args.video.clone() {
                                tokio::spawn(stream_video(conn.clone(), path));
                            }
                        }
                    }
                    Message::Input { .. } | Message::Scroll { .. } | Message::GlobalAction { .. } | Message::TypeText { .. } | Message::Key { .. } | Message::KeyframeRequest | Message::MirrorStop => {}
                    _ => {}
                }
            }
        }
    }
    conn.close(0u32.into(), b"bye");
    Ok(())
}

/// Streams an H.264 file as the phone's encoder would: one NAL unit a record,
/// slices about 30 a second.
async fn stream_video(conn: omalink_net::Conn, path: PathBuf) {
    let data = std::fs::read(&path).expect("video file");
    let mut send = conn.open_uni().await.expect("stream");
    omalink_net::write(&mut send, &Message::MirrorStream { width: 720, height: 1280 }).await.expect("header");
    // Start codes split the file into NAL units; each keeps its start code.
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            starts.push(if i > 0 && data[i - 1] == 0 { i - 1 } else { i });
            i += 3;
        } else {
            i += 1;
        }
    }
    starts.push(data.len());
    let mut sent = 0;
    for pair in starts.windows(2) {
        let nal = &data[pair[0]..pair[1]];
        let header = nal.iter().position(|&b| b == 1).map(|p| nal.get(p + 1).copied().unwrap_or(0) & 0x1f).unwrap_or(0);
        let flags = match header {
            7 | 8 => VideoFrame::CONFIG,
            5 => VideoFrame::KEY,
            _ => 0,
        };
        let frame = VideoFrame::header(nal.len(), sent * 33_000, flags);
        if send.write_all(&frame).await.is_err() || send.write_all(nal).await.is_err() {
            println!("video stream closed by the desktop");
            return;
        }
        if header == 1 || header == 5 {
            sent += 1;
            tokio::time::sleep(Duration::from_millis(33)).await;
        }
    }
    let _ = send.finish();
    println!("video sent: {sent} frames");
}
