//! # crush-net
//!
//! Portable, std-only network stack exposed as CRUSH host capabilities.
//! Replaces the libp2p / rcgen / time / tokio stack of `exo-mesh` with a
//! deterministic reactor that compiles in seconds.
//!
//! ## Capabilities: `tcp.*`, three independently granted groups
//!
//! The caps are named `tcp.*`, not `net.*`: `net.*` is the HTTP-client family
//! that `crush-run --net` grants wholesale (and that `@capabilities net` /
//! `capsule.toml` `net` would cover), and raw sockets must never ride along
//! with it (CRUSH-318). Hosts register them in groups:
//!
//! | register fn | caps | `crush-run` flag |
//! |---|---|---|
//! | [`register_client`] | `tcp.connect`, `tcp.tls_wrap` | `--tcp` |
//! | [`register_listen`] | `tcp.listen`, `tcp.accept` (being a server) | `--tcp-listen` |
//! | [`register_io`] | `tcp.send`, `tcp.recv`, `tcp.close`, `tcp.ping` | either of the above |
//!
//! Addresses are `tcp://<ip>:<port>` (a literal IP; there is no DNS lookup).
//! Connection ids are `Int`s; `tcp.send`/`tcp.recv` move UTF-8 `Str`s.
//!
//! ## Contents
//!
//! - [`caps`] — the `HostCap` implementations backed by [`TcpTransport`].
//! - [`transport`] — `AsyncRead` / `AsyncWrite` / `AsyncAccept` / `Transport`
//!   traits + a `parse_uri("tcp://host:port")` helper.
//! - [`codec`] — fixed-header TLV frame format with a 16 MiB hard ceiling +
//!   `encode_request` / `decode_request` for [`mesh_request::MeshRequest`].
//! - [`reactor`] — a single polling-thread reactor that drives registered
//!   [`Source`]s and wakes futures via `Waker`.
//! - [`tcp`] — [`TcpTransport`] / [`TcpConnection`] / [`TcpListener`] gluing
//!   the above together with `std::net::TcpStream`. The listener-side waker
//!   gap from Phase 2 is closed via `reactor::Source::try_accept` + a
//!   registered `Waker` on `Poll::Pending`.

pub mod caps;
pub mod codec;
pub mod mesh_request;
pub mod reactor;
pub mod tcp;
pub mod transport;

#[cfg(feature = "tls")]
pub use caps::TcpTlsWrapCap;
pub use caps::{
    ConnId, NetState, SharedConn, TcpAcceptCap, TcpCloseCap, TcpConnectCap, TcpListenCap,
    TcpPingCap, TcpRecvCap, TcpSendCap, build_state, register_client, register_io, register_listen,
};
pub use codec::{
    FRAME_TYPE_MESH_REQUEST, Frame, MAX_FRAME_SIZE, NetError, decode_request, encode_frame,
    encode_request, try_decode_frame,
};
pub use reactor::{PollerHandle, Reactor, Source, SourceId};
pub use tcp::{TcpConnection, TcpListener, TcpTransport};
pub use transport::{AsyncAccept, AsyncRead, AsyncWrite, Endpoint, Transport, parse_uri};

use crush_vm::HostCaps;

/// Register **every** `tcp.*` HostCap (client, listen and io) on `caps`,
/// with no gating. Constructs a default `Reactor` + `NetState` internally; to
/// share a `Reactor`, or to grant only some groups, use [`caps::build_state`]
/// with [`register_client`] / [`register_listen`] / [`register_io`].
pub fn register(caps: &mut HostCaps) {
    caps::register(caps, new_state());
}

/// A fresh shared connection table for the `register_*` functions: the three
/// groups of one program must share one so `tcp.accept`'s ids work in `tcp.recv`.
pub fn new_state() -> std::sync::Arc<std::sync::Mutex<NetState>> {
    build_state(std::sync::Arc::new(Reactor::new()))
}
