//! Two nodes on this machine: icon verification over the pairing protocol,
//! then a file each way over the link protocol. No relays, no internet.

use omalink_net::iroh::RelayMode;
use omalink_net::{accept_messages, id_bytes, open_messages, random32, read_header, receive_file, send_file, Node, Role};
use omalink_proto::net::{LINK_ALPN, PAIR_ALPN};
use omalink_proto::pair::{Chooser, Shower, PAIR};
use omalink_proto::{Message, PROTOCOL_VERSION};

async fn node(role: Role) -> Node {
    Node::start_with(random32(), role, RelayMode::Disabled).await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn pair_then_send_files_both_ways() {
    let desktop = node(Role::Desktop).await;
    let phone = node(Role::Phone).await;
    let desktop_addr = desktop.endpoint.addr();
    let (desktop_id, phone_id) = (id_bytes(&desktop.endpoint.id()), id_bytes(&phone.endpoint.id()));

    // Pairing: the phone dials, commits, and taps the icon.
    let desktop_side = tokio::spawn(async move {
        let (alpn, conn) = desktop.accept().await.unwrap().unwrap();
        assert_eq!(alpn, PAIR_ALPN);
        assert_eq!(id_bytes(&conn.remote_id()), phone_id);
        let (mut send, mut reader) = accept_messages(&conn).await.unwrap();
        assert!(matches!(reader.next().await.unwrap(), Some(Message::Hello { .. })));
        let Some(Message::VerifyCommit { purpose, commit }) = reader.next().await.unwrap() else { panic!() };
        let nonce = random32();
        let mut shower = Shower::new(&purpose, &desktop_id, &phone_id, &commit, &nonce).unwrap();
        omalink_net::write(&mut send, &Message::VerifyNonce { nonce: omalink_proto::pair::to_hex(&nonce) }).await.unwrap();
        let Some(Message::VerifyReveal { nonce }) = reader.next().await.unwrap() else { panic!() };
        let shown = shower.reveal(&nonce).unwrap().icon;
        let Some(Message::VerifyResult { ok }) = reader.next().await.unwrap() else { panic!() };
        omalink_net::write(&mut send, &Message::VerifyDone { ok }).await.unwrap();
        assert!(ok);
        // Closing now would throw away what hasn't been delivered; the phone
        // hangs up once it has read the answer.
        omalink_net::finish(&conn, send).await;
        (desktop, shown)
    });

    let conn = phone.dial_addr(desktop_addr.clone(), PAIR_ALPN).await.unwrap();
    let hello = Message::Hello { version: PROTOCOL_VERSION, name: "phone".into() };
    let (mut send, mut reader) = open_messages(&conn, &hello).await.unwrap();
    let mut chooser = Chooser::new(PAIR, &desktop_id, &phone_id, &random32()).unwrap();
    omalink_net::write(&mut send, &Message::VerifyCommit { purpose: PAIR.into(), commit: chooser.commitment() }).await.unwrap();
    let Some(Message::VerifyNonce { nonce }) = reader.next().await.unwrap() else { panic!() };
    let (reveal, sas) = chooser.nonce(&nonce).unwrap();
    omalink_net::write(&mut send, &Message::VerifyReveal { nonce: reveal }).await.unwrap();
    // The person looks at the desktop and taps the same icon.
    assert!(chooser.check(sas.icon));
    omalink_net::write(&mut send, &Message::VerifyResult { ok: true }).await.unwrap();
    assert_eq!(reader.next().await.unwrap(), Some(Message::VerifyDone { ok: true }));
    conn.close(0u32.into(), b"paired");
    let (desktop, shown) = desktop_side.await.unwrap();
    assert_eq!(shown, sas.icon);
    assert!(sas.choices.contains(&shown));

    // The link: a file from the phone, then one from the desktop.
    let payload: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    let expect = payload.clone();
    let desktop_side = tokio::spawn(async move {
        let (alpn, conn) = desktop.accept().await.unwrap().unwrap();
        assert_eq!(alpn, LINK_ALPN);
        let (_send, mut reader) = accept_messages(&conn).await.unwrap();
        assert!(matches!(reader.next().await.unwrap(), Some(Message::Hello { .. })));
        let mut recv = conn.accept_uni().await.unwrap();
        let Message::FileStart { id, name, size } = read_header(&mut recv).await.unwrap() else { panic!() };
        assert_eq!((id, name.as_str(), size), (1, "photo.jpg", expect.len() as i64));
        let mut got = Vec::new();
        receive_file(&mut recv, &mut got, size, |_| {}).await.unwrap();
        assert_eq!(got, expect);
        let back = b"from the desktop".to_vec();
        send_file(&conn, 2, "note.txt", back.len() as i64, back.as_slice(), |_| {}).await.unwrap();
        conn
    });
    let conn = phone.dial_addr(desktop_addr, LINK_ALPN).await.unwrap();
    let (_send, _reader) = open_messages(&conn, &hello).await.unwrap();
    let sent = send_file(&conn, 1, "photo.jpg", payload.len() as i64, payload.as_slice(), |_| {}).await.unwrap();
    assert_eq!(sent, payload.len() as u64);
    let mut recv = conn.accept_uni().await.unwrap();
    let Message::FileStart { name, size, .. } = read_header(&mut recv).await.unwrap() else { panic!() };
    let mut got = Vec::new();
    receive_file(&mut recv, &mut got, size, |_| {}).await.unwrap();
    assert_eq!((name.as_str(), got.as_slice()), ("note.txt", b"from the desktop".as_slice()));
    let _keep = desktop_side.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_truncated_file_is_an_error() {
    let a = node(Role::Desktop).await;
    let b = node(Role::Phone).await;
    let addr = a.endpoint.addr();
    let side = tokio::spawn(async move {
        let (_, conn) = a.accept().await.unwrap().unwrap();
        let mut recv = conn.accept_uni().await.unwrap();
        let Message::FileStart { size, .. } = read_header(&mut recv).await.unwrap() else { panic!() };
        let mut got = Vec::new();
        receive_file(&mut recv, &mut got, size, |_| {}).await
    });
    let conn = b.dial_addr(addr, LINK_ALPN).await.unwrap();
    // Claims 100 bytes, sends 10.
    send_file(&conn, 1, "short", 100, [0u8; 10].as_slice(), |_| {}).await.unwrap();
    assert!(side.await.unwrap().is_err());
}
