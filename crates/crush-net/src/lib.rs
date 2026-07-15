//! # crush-net
//!
//! Portable, std-only network stack exposed as CRUSH host capabilities.
//! Replaces the libp2p / rcgen / time / tokio stack of `exo-mesh` with a
//! deterministic reactor that compiles in seconds.
//!
//! ## Phase 3 contents (this commit)
//!
//! - [`caps`] — `register()` actually wires `net.connect` / `net.listen` /
//!   `net.accept` / `net.send` / `net.recv` / `net.close` / `net.ping` HostCaps
//!   backed by [`TcpTransport`].
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

pub use caps::{
    build_state, NetAcceptCap, NetCloseCap, NetConnectCap, NetListenCap, NetPingCap,
    NetRecvCap, NetSendCap, NetState, SharedConn, ConnId,
};
pub use codec::{
    decode_request, encode_frame, encode_request, try_decode_frame, Frame, NetError,
    FRAME_TYPE_MESH_REQUEST, MAX_FRAME_SIZE,
};
pub use reactor::{PollerHandle, Reactor, Source, SourceId};
pub use tcp::{TcpConnection, TcpListener, TcpTransport};
pub use transport::{
    parse_uri, AsyncAccept, AsyncRead, AsyncWrite, Endpoint, Transport,
};

use crush_lang_sdk::HostCaps;

/// Register the `net.*` HostCaps on the given [`HostCaps`] registry.
///
/// Constructs a default `Reactor` + `NetState` internally; if your host already
/// owns a `Reactor` and wants to share it across callbacks, use
/// [`caps::register`] directly with a state built via [`caps::build_state`].
pub fn register(caps: &mut HostCaps) {
    use std::sync::Arc;
    let reactor = Arc::new(crate::reactor::Reactor::new());
    let state = crate::caps::build_state(reactor);
    crate::caps::register(caps, state);
}
