//! TLS smoke test for `crush-net`.
//!
//! Only compiles when the `tls` feature is on:
//!     cargo test -p crush-net --features tls
//!
//! Exercises the Phase-4 deliverable end-to-end:
//!   * `rcgen` mints an in-process self-signed cert (SAN = "localhost").
//!   * A `std::net::TcpListener` accepts on 127.0.0.1:0; an acceptor thread drives a
//!     `rustls::ServerConnection` handshake over the accepted socket, reads a 4-byte
//!     "ping", writes a 4-byte "pong", then drops.
//!   * The main thread builds a `NetState`, constructs `NetTlsWrapCap` via
//!     `with_extra_roots(state.clone(), vec![self_signed_der])` so the client's
//!     `RootCertStore` includes the test cert as an extra trust anchor, calls
//!     `HostCap::call(&args)`, asserts the wrap returned a fresh `ConnId` AND that
//!     the original `ConnId` is no longer in `state.conns`, and drives the
//!     application-layer ping/pong round-trip through `state.tls_client_conns[new_id]`.

#![cfg(feature = "tls")]

use std::{
    io::{Read, Write},
    net::TcpListener as StdTcpListener,
    sync::{Arc, Mutex},
    thread,
};

use crush_lang_sdk::{HostCap, Value};
use crush_net::caps::{NetState, NetTlsWrapCap};

#[test]
fn net_tls_wrap_performs_handshake_and_probe() {
    // ---- server-side: mint self-signed cert, accept, handshake, send probe ----
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let cert_der = rustls::pki_types::CertificateDer::from(cert.cert.der().to_vec());
    let key_der =
        rustls::pki_types::PrivateKeyDer::Pkcs8(cert.key_pair.serialize_der().into());

    // Clone for the test-side trust anchor (server uses it via with_single_cert).
    let cert_der_for_store = cert_der.clone();

    let listener = StdTcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
    let port = listener.local_addr().unwrap().port();

    let acceptor = thread::spawn(move || {
        let (sock, _peer) = listener.accept().expect("accept");
        let cfg = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der], key_der)
            .expect("server config");
        let conn = rustls::ServerConnection::new(std::sync::Arc::new(cfg))
            .expect("server conn");
        let mut owned = rustls::StreamOwned::new(conn, sock);
        // Drive TLS handshake to completion.
        while owned.conn.is_handshaking() {
            if owned.conn.complete_io(&mut owned.sock).is_err() {
                return;
            }
        }
        // After handshake, read/write through `owned` so bytes are decrypted/encrypted.
        let mut buf = [0u8; 4];
        if owned.read_exact(&mut buf).is_ok() && &buf == b"ping" {
            let _ = owned.write_all(b"pong");
        }
    });

    // ---- client-side: dial via TcpTransport ----
    let reactor = Arc::new(crush_net::reactor::Reactor::new());
    let transport = crush_net::TcpTransport::new(reactor.clone());
    let conn_uri = format!("tcp://127.0.0.1:{port}");
    let conn = transport
        .connect_endpoint(&crush_net::transport::parse_uri(&conn_uri).unwrap())
        .expect("dial TLS peer for plain TCP pre-handshake");
    let pre_id: i64 = 1;
    let state = Arc::new(Mutex::new(NetState {
        transport,
        conns: [(pre_id, Arc::new(Mutex::new(conn)))].into_iter().collect(),
        listeners: Default::default(),
        next_id: pre_id,
        #[cfg(feature = "tls")]
        tls_client_conns: Default::default(),
    }));

    // ---- construct cap WITH the test cert as an extra trust anchor ----
    let cap = NetTlsWrapCap::with_extra_roots(state.clone(), vec![cert_der_for_store]);
    let result = cap.call(vec![
        Value::Int(pre_id),
        Value::Str("localhost".to_string()),
    ]);

    let new_id = match result {
        Ok(Some(Value::Int(id))) => id,
        Ok(Some(other)) => panic!("expected Value::Int, got {other:?}"),
        Ok(None) => panic!("net.tls_wrap returned None — should be Some(Int)"),
        Err(e) => panic!("net.tls_wrap errored: {e}"),
    };

    assert_ne!(new_id, pre_id, "tls_wrap must return a fresh ConnId");

    let gs = state.lock().unwrap();
    assert!(!gs.conns.contains_key(&pre_id), "after wrap, original conn-id {pre_id} must NOT be in state.conns");
    assert!(gs.tls_client_conns.contains_key(&new_id), "tls_client_conns must contain the new conn-id {new_id}");

    // Drive the application-layer ping/pong round-trip OUTSIDE the cap.
    // Order matters: do the ping/pong exchange first; join the server thread after,
    // because the server's `owned.read_exact` blocks on 4 decrypted bytes until we
    // write "ping" (and the server returns only after `write_all("pong")` completes).
    {
        let mut stream = gs.tls_client_conns.get(&new_id).unwrap().lock().unwrap();
        stream.write_all(b"ping").expect("client write ping");
        let mut buf = [0u8; 4];
        stream.read_exact(&mut buf).expect("client read pong");
        assert_eq!(&buf, b"pong", "round-trip expected pong, got {buf:?}");
    }
    // MutexGuard on `gs` is now dropped (via the inner `{}` block above); lock state
    // once more briefly to join without holding the state-mutex during the join.
    drop(gs);
    acceptor.join().expect("acceptor thread did not panic");
}
