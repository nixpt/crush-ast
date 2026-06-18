//! Async I/O abstractions over `std::task`.
//!
//! Hand-rolled so we don't need `tokio::io` or `futures::io`. The trait shapes
//! are deliberately close to the well-known ones so a Phase-4 swap-in to
//! tokio or futures is mechanical.

use std::{
    io,
    net::IpAddr,
    pin::Pin,
    task::{Context, Poll},
};

pub trait AsyncRead {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>>;
}

pub trait AsyncWrite {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>>;
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>>;
    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>>;
}

pub trait AsyncAccept {
    type Conn;
    fn poll_accept(self: Pin<&mut Self>, cx: &mut Context<'_>)
        -> Poll<io::Result<Self::Conn>>;
}

pub trait Transport: Send + Sync + 'static {
    type Connection: AsyncRead + AsyncWrite + Unpin + Send;
    type Listener: AsyncAccept<Conn = Self::Connection> + Unpin + Send;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint(pub IpAddr, pub u16);

/// Parse `tcp://host:port`. Host must be an IP literal (v4 or v6); DNS is the
/// caller's responsibility via `std::net::ToSocketAddrs`.
pub fn parse_uri(uri: &str) -> io::Result<Endpoint> {
    let rest = uri.strip_prefix("tcp://").ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, format!("not a tcp:// URI: {uri}"))
    })?;
    let (host_raw, port_raw) = rest.rsplit_once(':').ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("missing port in URI: {uri}"),
        )
    })?;
    let host = host_raw.trim_matches(|c| c == '[' || c == ']');
    let port: u16 = port_raw.parse().map_err(|e: std::num::ParseIntError| {
        io::Error::new(io::ErrorKind::InvalidInput, format!("invalid port: {e}"))
    })?;
    let ip: IpAddr = host.parse().map_err(|e: std::net::AddrParseError| {
        io::Error::new(io::ErrorKind::InvalidInput, format!("invalid IP: {e}"))
    })?;
    Ok(Endpoint(ip, port))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn parse_tcp_uri_v4() {
        let e = parse_uri("tcp://127.0.0.1:9000").unwrap();
        assert_eq!(e, Endpoint(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9000));
    }

    #[test]
    fn parse_tcp_uri_v6() {
        let e = parse_uri("tcp://[::1]:9001").unwrap();
        assert_eq!(e, Endpoint(IpAddr::V6(Ipv6Addr::LOCALHOST), 9001));
    }

    #[test]
    fn reject_non_tcp_scheme() {
        assert!(parse_uri("udp://127.0.0.1:80").is_err());
    }

    #[test]
    fn reject_missing_port() {
        assert!(parse_uri("tcp://127.0.0.1").is_err());
    }
}
