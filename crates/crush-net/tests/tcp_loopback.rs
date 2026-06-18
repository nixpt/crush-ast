//! 127.0.0.1 TCP loopback test. Binds a `std::net::TcpListener` on an
//! OS-assigned port, spawns an acceptor thread, connects from the main
//! thread using `crush_net::TcpTransport`, sends a TLV frame containing a
//! `MeshRequest`, decodes on the listener side, asserts the exchange.
//!
//! Uses blocking `std::net::TcpListener::accept` on the acceptor thread; the
//! connector uses `crush_net::TcpConnection::blocking_write` so the test
//! doesn't have to synchronize with the polling reactor's 10 ms wake cadence.
//! We deliberately do NOT spawn `Reactor::spawn_poller` — blocking_write is
//! synchronous and the poller thread would leak on test exit.

use std::{
    io::Read,
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener as StdTcpListener},
    sync::{mpsc, Arc},
    thread,
    time::Duration,
};

use mesh_proto::MeshRequest;
use serde_json::json;

#[test]
fn tcp_loopback_with_mesh_request_tlv() {
    // Bind std listener on OS-assigned port.
    let std_listener =
        StdTcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).expect("bind");
    let bound_port = std_listener.local_addr().expect("local_addr").port();
    let endpoint = crush_net::Endpoint(IpAddr::V4(Ipv4Addr::LOCALHOST), bound_port);

    // Construct the crush-net transport (no poller spawned — see module docs).
    let reactor = Arc::new(crush_net::Reactor::new());
    let transport = crush_net::TcpTransport::new(Arc::clone(&reactor));

    // Single-shot mpsc: acceptor reports the decoded MeshRequest back to main.
    let (decoded_tx, decoded_rx) = mpsc::channel::<MeshRequest>();

    // Spawn the acceptor. `move` captures std_listener by ownership.
    let acceptor = thread::spawn(move || {
        if let Ok((mut stream, _peer)) = std_listener.accept() {
            let mut buf = vec![0u8; 8192];
            let n = stream.read(&mut buf).unwrap_or(0);
            buf.truncate(n);
            let (req, _consumed): (mesh_proto::MeshRequest, usize) =
                crush_net::decode_request(&buf).expect("decode");
            decoded_tx.send(req).ok();
        }
    });

    // Connector: connect via crush-net and write a TLV frame.
    let conn = transport.connect_endpoint(&endpoint).expect("connect");

    let original = MeshRequest {
        id: "req-loopback".to_string(),
        method: "loopback-ping".to_string(),
        params: json!({ "ok": true, "n": 7 }),
        caller_did: "did:key:zLoopbackTest".to_string(),
        correlation_id: Some("loop-corr".to_string()),
    };
    let bytes = crush_net::encode_request(&original).expect("encode");
    let written = conn.blocking_write(&bytes).expect("blocking_write");
    assert_eq!(written, bytes.len(), "blocking_write must succeed end-to-end");

    let received = decoded_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("acceptor must send decoded MeshRequest within 5s");
    assert_eq!(received.id, "req-loopback");
    assert_eq!(received.method, "loopback-ping");
    assert_eq!(received.caller_did, "did:key:zLoopbackTest");
    assert_eq!(received.correlation_id.as_deref(), Some("loop-corr"));
    assert_eq!(received.params, json!({ "ok": true, "n": 7 }));

    drop(conn);
    acceptor.join().ok();
}
