//! Crush host-capability registration for the `net.*` API surface.
//!
//! All seven caps share `Arc<Mutex<NetState>>` over the `TcpTransport`,
//! a `HashMap<ConnId, SharedConn>` of live connections, and a listener
//! registry. Phase-3 caps are synchronous (blocking-accept on the listener
//! side); the async path is wired through `reactor::Source::try_accept`.

use std::{collections::HashMap, sync::{Arc, Mutex}};

use crush_lang_sdk::{HostCap, HostCapSpec, HostCaps, Value};

use crate::{
    reactor::Reactor,
    tcp::{TcpConnection, TcpListener, TcpTransport},
    transport::parse_uri,
};

pub type ConnId = i64;
pub type SharedConn = Arc<Mutex<TcpConnection>>;

pub struct NetState {
    pub transport: TcpTransport,
    pub conns: HashMap<ConnId, SharedConn>,
    pub listeners: HashMap<ConnId, Arc<Mutex<TcpListener>>>,
    pub next_id: ConnId,
    #[cfg(feature = "tls")]
    pub tls_client_conns: HashMap<
        ConnId,
        std::sync::Arc<
            std::sync::Mutex<
                rustls::StreamOwned<rustls::ClientConnection, std::net::TcpStream>,
            >,
        >,
    >,
}

impl NetState {
    pub fn new(reactor: Arc<Reactor>) -> Self {
        Self {
            transport: TcpTransport::new(reactor),
            conns: HashMap::new(),
            listeners: HashMap::new(),
            next_id: 1,
            #[cfg(feature = "tls")]
            tls_client_conns: HashMap::new(),
        }
    }
    pub fn next(&mut self) -> ConnId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// Construct a fresh `NetState` wrapped in `Arc<Mutex<>>` for sharing across caps.
pub fn build_state(reactor: Arc<Reactor>) -> Arc<Mutex<NetState>> {
    Arc::new(Mutex::new(NetState::new(reactor)))
}

fn str_arg(args: &[Value], i: usize) -> Result<String, String> {
    match args.get(i) {
        Some(Value::Str(s)) => Ok(s.clone()),
        Some(v) => Err(format!("expected str at {i}, got {v:?}")),
        None => Err(format!("missing arg at {i}")),
    }
}

fn id_arg(args: &[Value], i: usize) -> Result<ConnId, String> {
    match args.get(i) {
        Some(Value::Int(i)) => Ok(*i),
        Some(v) => Err(format!("expected i64 at {i}, got {v:?}")),
        None => Err(format!("missing arg at {i}")),
    }
}

pub struct NetConnectCap(pub Arc<Mutex<NetState>>);
impl HostCap for NetConnectCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: "net.connect".into(), argc: Some(1), returns: true }
    }
    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let uri = str_arg(&args, 0)?;
        let ep = parse_uri(&uri).map_err(|e| format!("net.connect: uri: {e}"))?;
        let mut s = self.0.lock().unwrap();
        let conn = s.transport.connect_endpoint(&ep)
            .map_err(|e| format!("net.connect: {e}"))?;
        let id = s.next();
        s.conns.insert(id, Arc::new(Mutex::new(conn)));
        Ok(Some(Value::Int(id)))
    }
}

pub struct NetListenCap(pub Arc<Mutex<NetState>>);
impl HostCap for NetListenCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: "net.listen".into(), argc: Some(1), returns: true }
    }
    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let uri = str_arg(&args, 0)?;
        let ep = parse_uri(&uri).map_err(|e| format!("net.listen: uri: {e}"))?;
        let mut s = self.0.lock().unwrap();
        let listener = s.transport.listen_endpoint(&ep)
            .map_err(|e| format!("net.listen: {e}"))?;
        let id = s.next();
        s.listeners.insert(id, Arc::new(Mutex::new(listener)));
        Ok(Some(Value::Int(id)))
    }
}

pub struct NetAcceptCap(pub Arc<Mutex<NetState>>);
impl HostCap for NetAcceptCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: "net.accept".into(), argc: Some(1), returns: true }
    }
    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let listener_id = id_arg(&args, 0)?;
        let s = self.0.lock().unwrap();
        let listener = s.listeners.get(&listener_id)
            .ok_or_else(|| format!("net.accept: unknown listener {listener_id}"))?
            .clone();
        let tcp_conn = listener.lock().unwrap().accept_blocking()
            .map_err(|e| format!("net.accept: {e}"))?;
        drop(s);
        let mut s = self.0.lock().unwrap();
        let id = s.next();
        s.conns.insert(id, Arc::new(Mutex::new(tcp_conn)));
        Ok(Some(Value::Int(id)))
    }
}

pub struct NetSendCap(pub Arc<Mutex<NetState>>);
impl HostCap for NetSendCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: "net.send".into(), argc: Some(2), returns: true }
    }
    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let id = id_arg(&args, 0)?;
        let bytes = str_arg(&args, 1)?.into_bytes();
        let s = self.0.lock().unwrap();
        let conn = s.conns.get(&id)
            .ok_or_else(|| format!("net.send: unknown conn {id}"))?;
        let n = conn.lock().unwrap().blocking_write(&bytes)
            .map_err(|e| format!("net.send: {e}"))?;
        Ok(Some(Value::Int(n as i64)))
    }
}

pub struct NetRecvCap(pub Arc<Mutex<NetState>>);
impl HostCap for NetRecvCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: "net.recv".into(), argc: Some(1), returns: true }
    }
    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let id = id_arg(&args, 0)?;
        let s = self.0.lock().unwrap();
        let conn = s.conns.get(&id)
            .ok_or_else(|| format!("net.recv: unknown conn {id}"))?;
        let mut buf = vec![0u8; 16 * 1024];
        let n = conn.lock().unwrap().blocking_read(&mut buf)
            .map_err(|e| format!("net.recv: {e}"))?;
        buf.truncate(n);
        let s = String::from_utf8(buf).map_err(|e| format!("net.recv: utf8: {e}"))?;
        Ok(Some(Value::Str(s)))
    }
}

pub struct NetCloseCap(pub Arc<Mutex<NetState>>);
impl HostCap for NetCloseCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: "net.close".into(), argc: Some(1), returns: false }
    }
    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let id = id_arg(&args, 0)?;
        let mut s = self.0.lock().unwrap();
        s.conns.remove(&id);
        Ok(None)
    }
}

pub struct NetPingCap;
impl HostCap for NetPingCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: "net.ping".into(), argc: Some(1), returns: true }
    }
    fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
        Ok(Some(Value::Str("pong".to_string())))
    }
}

/// Register all seven `net.*` HostCaps on the given [`HostCaps`] registry.
pub fn register(caps: &mut HostCaps, state: Arc<Mutex<NetState>>) {
    caps.register(Box::new(NetPingCap));
    caps.register(Box::new(NetConnectCap(Arc::clone(&state))));
    caps.register(Box::new(NetListenCap(Arc::clone(&state))));
    caps.register(Box::new(NetAcceptCap(Arc::clone(&state))));
    caps.register(Box::new(NetSendCap(Arc::clone(&state))));
    caps.register(Box::new(NetRecvCap(Arc::clone(&state))));
    caps.register(Box::new(NetCloseCap(Arc::clone(&state))));
    #[cfg(feature = "tls")]
    caps.register(Box::new(NetTlsWrapCap::new(Arc::clone(&state))));
}

#[cfg(feature = "tls")]
pub struct NetTlsWrapCap {
    state: std::sync::Arc<std::sync::Mutex<NetState>>,
    /// Optional additional trust anchors.
    ///
    /// When non-empty, `handshake()` places them in the client `RootCertStore`
    /// ALONGSIDE `webpki_roots::TLS_SERVER_ROOTS`. Tests use this to inject the
    /// in-process self-signed cert so the smoke-test handshake validates
    /// locally. Production callers leave it empty.
    #[cfg_attr(not(test), allow(dead_code))]
    pub extra_roots: Vec<rustls::pki_types::CertificateDer<'static>>,
}

#[cfg(feature = "tls")]
impl NetTlsWrapCap {
    pub fn new(state: std::sync::Arc<std::sync::Mutex<NetState>>) -> Self {
        Self {
            state,
            extra_roots: Vec::new(),
        }
    }

    /// Construct with extra trust anchors (used by tests for self-signed certs;
    /// would also be used by production callers that want to bundle an
    /// enterprise CA at startup).
    pub fn with_extra_roots(
        state: std::sync::Arc<std::sync::Mutex<NetState>>,
        extra_roots: Vec<rustls::pki_types::CertificateDer<'static>>,
    ) -> Self {
        Self { state, extra_roots }
    }

    /// Drive a synchronous rustls client handshake on the connection referenced by
    /// `conn_id`. The original `TcpConnection` is REMOVED from `state.conns`; the
    /// upgraded `rustls::StreamOwned<ClientConnection, TcpStream>` is stored under
    /// a fresh `ConnId` in `state.tls_client_conns`. Does NOT perform any
    /// application-layer round-trip — that is the caller's responsibility.
    fn handshake(
        state: &std::sync::Arc<std::sync::Mutex<NetState>>,
        conn_id: ConnId,
        sni: &'static str,
        extra_roots: &[rustls::pki_types::CertificateDer<'static>],
    ) -> Result<ConnId, String> {
        use rustls::pki_types::ServerName;

        let mut roots = rustls::RootCertStore::empty();
        // Mozilla root store (production path).
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        // Test / enterprise extra anchors (none for production callers).
        for r in extra_roots {
            roots
                .add(r.clone())
                .map_err(|e| format!("extra trust anchor: {e}"))?;
        }
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let server_name = ServerName::try_from(sni)
            .map_err(|e| format!("invalid SNI `{sni}`: {e}"))?;

        // Take the original conn out of state.conns; clone the TcpStream; drop
        // the lock; drive the handshake; re-take the lock to alloc + insert.
        let shared = {
            let mut g = state
                .lock()
                .map_err(|e| format!("state lock poisoned: {e}"))?;
            g.conns
                .remove(&conn_id)
                .ok_or_else(|| format!("conn-id {conn_id} not registered in state.conns"))?
        };

        let stream = shared
            .lock()
            .map_err(|e| format!("conn lock poisoned: {e}"))?
            .clone_stream()
            .map_err(|e| format!("clone_stream: {e}"))?;

        let conn = rustls::ClientConnection::new(std::sync::Arc::new(config), server_name)
            .map_err(|e| format!("client conn init: {e}"))?;
        let mut owned = rustls::StreamOwned::new(conn, stream);
        while owned.conn.is_handshaking() {
            // rustls 0.23: complete_io() lives on the connection, not on StreamOwned.
            // We pass `&mut owned.sock` (the in-crate TcpStream) for it to drive.
            owned
                .conn
                .complete_io(&mut owned.sock)
                .map_err(|e| format!("handshake io: {e}"))?;
        }

        // Single critical section: alloc next_id + insert tls_client_conns.
        let new_id = {
            let mut g = state
                .lock()
                .map_err(|e| format!("state lock poisoned: {e}"))?;
            g.next_id += 1;
            let id = g.next_id;
            g.tls_client_conns
                .insert(id, std::sync::Arc::new(std::sync::Mutex::new(owned)));
            id
        };
        Ok(new_id)
    }
}

#[cfg(feature = "tls")]
impl HostCap for NetTlsWrapCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "net.tls_wrap".into(),
            argc: Some(2),
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let conn_id = match args.first() {
            Some(Value::Int(i)) => *i,
            _ => return Err("net.tls_wrap: arg 0 must be Int (conn-id)".into()),
        };
        let sni = match args.get(1) {
            Some(Value::Str(s)) => s.clone(),
            _ => return Err("net.tls_wrap: arg 1 must be Str (SNI hostname)".into()),
        };
        // rustls 0.23 requires `ServerName<'static>` for `ClientConnection::new`.
        // The cap receives an owned String from the bytecode argument; we leak the
        // `&str` into 'static storage so the borrow survives into the connection.
        // Phase-5 hardening: cache `ServerName<'static>` per distinct sni string
        // rather than calling `Box::leak` per cap call.
        let sni_static: &'static str = Box::leak(sni.into_boxed_str());
        let new_id = Self::handshake(&self.state, conn_id, sni_static, &self.extra_roots)?;
        Ok(Some(Value::Int(new_id)))
    }
}
