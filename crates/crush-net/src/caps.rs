//! Crush host-capability registration for the `tcp.*` API surface.
//!
//! The caps are registered in three independently grantable groups (CRUSH-318),
//! so a host can hand out client sockets without handing out a server:
//!
//! | group | caps | what it lets a program do |
//! |---|---|---|
//! | [`register_client`] | `tcp.connect`, `tcp.tls_wrap` | open outbound connections |
//! | [`register_listen`] | `tcp.listen`, `tcp.accept` | **be a server**: bind a port, take inbound connections |
//! | [`register_io`] | `tcp.send`, `tcp.recv`, `tcp.close`, `tcp.ping` | use a connection id the program already holds |
//!
//! `register_io` carries no authority of its own: an id only exists if a client
//! or listen cap produced it. `crush-run` registers it whenever either of the
//! other two groups is granted (`--tcp`, `--tcp-listen`).
//!
//! All caps share `Arc<Mutex<NetState>>` over the `TcpTransport`,
//! a `HashMap<ConnId, SharedConn>` of live connections, and a listener
//! registry. Caps are synchronous (blocking-accept on the listener
//! side); the async path is wired through `reactor::Source::try_accept`.

use std::{collections::HashMap, sync::{Arc, Mutex}};
#[cfg(feature = "tls")]
use std::sync::OnceLock;
#[cfg(feature = "tls")]
use rustls::pki_types::ServerName;

/// Cache of leaked SNI `&'static str`s, indexed by the originating `&str`.
/// Each distinct SNI is leaked at most once across the program lifetime
/// (vs. once per `TcpTlsWrapCap::call` invocation under the prior
/// `Box::leak`-per-call shape). The leak cost falls from `O(calls)` to
/// `O(distinct SNIs)`; re-parsing via `ServerName::try_from(&str)` is cheap
/// and runs on every call to produce a fresh `ServerName<'static>`.
///
/// Bad (unparseable) SNIs still leak one allocation per crash lifecycle;
/// accepted as a bounded misallocation to keep `call`'s API shape unchanged.
#[cfg(feature = "tls")]
static SNI_CACHE: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();

#[cfg(feature = "tls")]
fn cached_sni(sni: &str) -> &'static str {
    ServerName::try_from(sni).expect("invalid SNI before cache leak");
    let cache = SNI_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().expect("SNI_CACHE mutex poisoned");
    if let Some(&cached) = guard.get(sni) {
        return cached;
    }
    let leaked: &'static str = Box::leak(sni.to_owned().into_boxed_str());
    guard.insert(sni.to_owned(), leaked);
    leaked
}

use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};

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

pub struct TcpConnectCap(pub Arc<Mutex<NetState>>);
impl HostCap for TcpConnectCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: "tcp.connect".into(), argc: Some(1), returns: true }
    }
    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let uri = str_arg(&args, 0)?;
        let ep = parse_uri(&uri).map_err(|e| format!("tcp.connect: uri: {e}"))?;
        let mut s = self.0.lock().unwrap();
        let conn = s.transport.connect_endpoint(&ep)
            .map_err(|e| format!("tcp.connect: {e}"))?;
        let id = s.next();
        s.conns.insert(id, Arc::new(Mutex::new(conn)));
        Ok(Some(Value::Int(id)))
    }
}

pub struct TcpListenCap(pub Arc<Mutex<NetState>>);
impl HostCap for TcpListenCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: "tcp.listen".into(), argc: Some(1), returns: true }
    }
    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let uri = str_arg(&args, 0)?;
        let ep = parse_uri(&uri).map_err(|e| format!("tcp.listen: uri: {e}"))?;
        let mut s = self.0.lock().unwrap();
        let listener = s.transport.listen_endpoint(&ep)
            .map_err(|e| format!("tcp.listen: {e}"))?;
        let id = s.next();
        s.listeners.insert(id, Arc::new(Mutex::new(listener)));
        Ok(Some(Value::Int(id)))
    }
}

pub struct TcpAcceptCap(pub Arc<Mutex<NetState>>);
impl HostCap for TcpAcceptCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: "tcp.accept".into(), argc: Some(1), returns: true }
    }
    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let listener_id = id_arg(&args, 0)?;
        let listener = {
            let s = self.0.lock().unwrap();
            s.listeners.get(&listener_id)
                .ok_or_else(|| format!("tcp.accept: unknown listener {listener_id}"))?
                .clone()
        };
        // The state lock is NOT held across the blocking accept.
        let tcp_conn = listener.lock().unwrap().accept_blocking()
            .map_err(|e| format!("tcp.accept: {e}"))?;
        let mut s = self.0.lock().unwrap();
        let id = s.next();
        s.conns.insert(id, Arc::new(Mutex::new(tcp_conn)));
        Ok(Some(Value::Int(id)))
    }
}

pub struct TcpSendCap(pub Arc<Mutex<NetState>>);
impl HostCap for TcpSendCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: "tcp.send".into(), argc: Some(2), returns: true }
    }
    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let id = id_arg(&args, 0)?;
        let bytes = str_arg(&args, 1)?.into_bytes();
        #[cfg(feature = "tls")]
        {
            let tls = self.0.lock().unwrap().tls_client_conns.get(&id).cloned();
            if let Some(tls) = tls {
                use std::io::Write;
                let mut g = tls.lock().unwrap();
                g.write_all(&bytes).and_then(|_| g.flush())
                    .map_err(|e| format!("tcp.send: {e}"))?;
                return Ok(Some(Value::Int(bytes.len() as i64)));
            }
        }
        let s = self.0.lock().unwrap();
        let conn = s.conns.get(&id)
            .ok_or_else(|| format!("tcp.send: unknown conn {id}"))?;
        let n = conn.lock().unwrap().blocking_write(&bytes)
            .map_err(|e| format!("tcp.send: {e}"))?;
        Ok(Some(Value::Int(n as i64)))
    }
}

pub struct TcpRecvCap(pub Arc<Mutex<NetState>>);
impl HostCap for TcpRecvCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: "tcp.recv".into(), argc: Some(1), returns: true }
    }
    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let id = id_arg(&args, 0)?;
        let mut buf = vec![0u8; 16 * 1024];
        #[cfg(feature = "tls")]
        let tls_n = {
            let tls = self.0.lock().unwrap().tls_client_conns.get(&id).cloned();
            match tls {
                Some(tls) => {
                    use std::io::Read;
                    let n = tls.lock().unwrap().read(&mut buf)
                        .map_err(|e| format!("tcp.recv: {e}"))?;
                    Some(n)
                }
                None => None,
            }
        };
        #[cfg(not(feature = "tls"))]
        let tls_n: Option<usize> = None;
        let n = match tls_n {
            Some(n) => n,
            None => {
                let s = self.0.lock().unwrap();
                let conn = s.conns.get(&id)
                    .ok_or_else(|| format!("tcp.recv: unknown conn {id}"))?
                    .clone();
                drop(s);
                conn.lock().unwrap().blocking_read(&mut buf)
                    .map_err(|e| format!("tcp.recv: {e}"))?
            }
        };
        buf.truncate(n);
        let s = String::from_utf8(buf).map_err(|e| format!("tcp.recv: utf8: {e}"))?;
        Ok(Some(Value::Str(s)))
    }
}

pub struct TcpCloseCap(pub Arc<Mutex<NetState>>);
impl HostCap for TcpCloseCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: "tcp.close".into(), argc: Some(1), returns: false }
    }
    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let id = id_arg(&args, 0)?;
        let mut s = self.0.lock().unwrap();
        s.conns.remove(&id);
        #[cfg(feature = "tls")]
        s.tls_client_conns.remove(&id);
        Ok(None)
    }
}

pub struct TcpPingCap;
impl HostCap for TcpPingCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: "tcp.ping".into(), argc: Some(1), returns: true }
    }
    fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
        Ok(Some(Value::Str("pong".to_string())))
    }
}

/// Outbound sockets: `tcp.connect` and (with the `tls` feature) `tcp.tls_wrap`.
/// Granted by `crush-run --tcp`.
pub fn register_client(caps: &mut HostCaps, state: Arc<Mutex<NetState>>) {
    caps.register(Box::new(TcpConnectCap(Arc::clone(&state))));
    #[cfg(feature = "tls")]
    caps.register(Box::new(TcpTlsWrapCap::new(state)));
    #[cfg(not(feature = "tls"))]
    let _ = state;
}

/// Server sockets: `tcp.listen` and `tcp.accept`. A stronger grant than
/// [`register_client`] (it makes the program reachable from other hosts);
/// granted by `crush-run --tcp-listen`.
pub fn register_listen(caps: &mut HostCaps, state: Arc<Mutex<NetState>>) {
    caps.register(Box::new(TcpListenCap(Arc::clone(&state))));
    caps.register(Box::new(TcpAcceptCap(state)));
}

/// Operations on an existing connection id: `tcp.send`, `tcp.recv`,
/// `tcp.close`, plus the pure `tcp.ping`. Needed by both of the other groups.
pub fn register_io(caps: &mut HostCaps, state: Arc<Mutex<NetState>>) {
    caps.register(Box::new(TcpPingCap));
    caps.register(Box::new(TcpSendCap(Arc::clone(&state))));
    caps.register(Box::new(TcpRecvCap(Arc::clone(&state))));
    caps.register(Box::new(TcpCloseCap(state)));
}

/// Register every `tcp.*` HostCap (client + listen + io), ungated. For hosts
/// that apply their own policy; `crush-run` uses the three group functions.
pub fn register(caps: &mut HostCaps, state: Arc<Mutex<NetState>>) {
    register_io(caps, Arc::clone(&state));
    register_client(caps, Arc::clone(&state));
    register_listen(caps, state);
}

#[cfg(feature = "tls")]
pub struct TcpTlsWrapCap {
    state: std::sync::Arc<std::sync::Mutex<NetState>>,
    /// Optional additional trust anchors.
    ///
    /// When non-empty, `handshake()` places them in the client `RootCertStore`
    /// ALONGSIDE `webpki_roots::TLS_SERVER_ROOTS`. Tests use this to inject the
    /// In-process self-signed cert so the smoke-test handshake validates
    /// locally. Production callers leave it empty. Crate-internal
    /// visibility (production); the integration test in `tests/tls_smoke.rs`
    /// routes through the public `TcpTlsWrapCap::with_extra_roots(...)`
    /// constructor rather than reading this field directly.
    pub(crate) extra_roots: Vec<rustls::pki_types::CertificateDer<'static>>,
    }

#[cfg(feature = "tls")]
impl TcpTlsWrapCap {
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
            let id = g.next();
            g.tls_client_conns
                .insert(id, std::sync::Arc::new(std::sync::Mutex::new(owned)));
            id
        };
        Ok(new_id)
    }
}

#[cfg(feature = "tls")]
impl HostCap for TcpTlsWrapCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "tcp.tls_wrap".into(),
            argc: Some(2),
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let conn_id = match args.first() {
            Some(Value::Int(i)) => *i,
            _ => return Err("tcp.tls_wrap: arg 0 must be Int (conn-id)".into()),
        };
        let sni = match args.get(1) {
            Some(Value::Str(s)) => s.clone(),
            _ => return Err("tcp.tls_wrap: arg 1 must be Str (SNI hostname)".into()),
        };
        // rustls 0.23 requires `ServerName<'static>` for `ClientConnection::new`.
        // The cap receives an owned String from the bytecode argument; we leak the
        // `&str` into 'static storage so the borrow survives into the connection.
        // Phase-5 hardening: cache `ServerName<'static>` per distinct sni string
        // rather than calling `Box::leak` per cap call.
        let sni_static: &'static str = cached_sni(&sni);
        let new_id = Self::handshake(&self.state, conn_id, sni_static, &self.extra_roots)?;
        Ok(Some(Value::Int(new_id)))
    }
}
