//! `std::net::TcpStream`-backed `Transport` implementation.
//!
//! Phase 3 changes:
//!   - `TcpListener` now exposes `id: SourceId` + `impl Source` so the
//!     reactor's polling thread can wake a parked `poll_accept` caller when a
//!     connection arrives.
//!   - `try_accept()` returns `Ok(Some(TcpStream))` on a fresh accept,
//!     `Ok(None)` on `WouldBlock`, or `Err` on real I/O errors.
//!   - `accept_blocking()` is the synchronous variant the Phase-3 HostCaps
//!     use; it toggles non-blocking briefly to drain one connection.

use std::{
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener as StdTcpListener, TcpStream, ToSocketAddrs},
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    task::{Context, Poll},
};

use crate::{
    reactor::{Reactor, Source, SourceId},
    transport::{AsyncAccept, AsyncRead, AsyncWrite, Endpoint, Transport},
};

pub struct TcpTransport {
    reactor: Arc<Reactor>,
}

impl TcpTransport {
    pub fn new(reactor: Arc<Reactor>) -> Self {
        Self { reactor }
    }

    pub fn connect<A: ToSocketAddrs>(&self, addr: A) -> io::Result<TcpConnection> {
        let stream = TcpStream::connect(addr)?;
        stream.set_nonblocking(true)?;
        Ok(TcpConnection::wrap(stream, Arc::clone(&self.reactor)))
    }

    pub fn listen<A: ToSocketAddrs>(&self, addr: A) -> io::Result<TcpListener> {
        let std_listener = StdTcpListener::bind(addr)?;
        std_listener.set_nonblocking(true)?;
        Ok(TcpListener {
            inner: Arc::new(Mutex::new(std_listener)),
            reactor: Arc::clone(&self.reactor),
            id: next_source_id(),
        })
    }

    pub fn connect_endpoint(&self, e: &Endpoint) -> io::Result<TcpConnection> {
        self.connect(SocketAddr::new(e.0, e.1))
    }

    pub fn listen_endpoint(&self, e: &Endpoint) -> io::Result<TcpListener> {
        self.listen(SocketAddr::new(e.0, e.1))
    }
}

impl Transport for TcpTransport {
    type Connection = TcpConnection;
    type Listener = TcpListener;
}

pub struct TcpConnection {
    stream: Arc<Mutex<TcpStream>>,
    reactor: Arc<Reactor>,
    id: SourceId,
}

impl TcpConnection {
    /// Clone the underlying `std::net::TcpStream`.
    ///
    /// Used by `NetTlsWrapCap` to hand an independent owned stream to
    /// `rustls::StreamOwned` without disturbing the reactor-registered
    /// original. The cloned stream is set to blocking so
    /// `rustls::ClientConnection::complete_io` blocks until the handshake
    /// makes progress.
    pub fn clone_stream(&self) -> std::io::Result<std::net::TcpStream> {
        let s = self
            .stream
            .lock()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("conn lock poisoned: {e}")))?;
        let cloned = s.try_clone()?;
        cloned.set_nonblocking(false)?;
        Ok(cloned)
    }

    fn wrap(stream: TcpStream, reactor: Arc<Reactor>) -> Self {
        Self {
            stream: Arc::new(Mutex::new(stream)),
            reactor,
            id: next_source_id(),
        }
    }

    pub fn blocking_read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let mut s = self.stream.lock().unwrap();
        s.set_nonblocking(false)?;
        let n = s.read(buf)?;
        s.set_nonblocking(true)?;
        Ok(n)
    }

    pub fn blocking_write(&self, buf: &[u8]) -> io::Result<usize> {
        let mut s = self.stream.lock().unwrap();
        s.set_nonblocking(false)?;
        let n = s.write(buf)?;
        s.set_nonblocking(true)?;
        Ok(n)
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.stream.lock().unwrap().peer_addr()
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.stream.lock().unwrap().local_addr()
    }
}

impl Source for TcpConnection {
    fn id(&self) -> SourceId { self.id }
    fn try_read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.lock().unwrap().read(buf)
    }
    fn try_write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.lock().unwrap().write(buf)
    }
}

impl AsyncRead for TcpConnection {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let me = self.get_mut();
        let attempt = me.stream.lock().unwrap().read(buf);
        match attempt {
            Ok(n) => Poll::Ready(Ok(n)),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                me.reactor.register(
                    Box::new(TcpClone {
                        id: me.id,
                        stream: Arc::clone(&me.stream),
                    }),
                    cx.waker().clone(),
                );
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

impl AsyncWrite for TcpConnection {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let me = self.get_mut();
        match me.stream.lock().unwrap().write(buf) {
            Ok(n) => Poll::Ready(Ok(n)),
            Err(e) => Poll::Ready(Err(e)),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let me = self.get_mut();
        me.reactor.unregister(me.id);
        Poll::Ready(Ok(()))
    }
}

struct TcpClone {
    id: SourceId,
    stream: Arc<Mutex<TcpStream>>,
}

impl Source for TcpClone {
    fn id(&self) -> SourceId { self.id }
    fn try_read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.lock().unwrap().read(buf)
    }
    fn try_write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.lock().unwrap().write(buf)
    }
}

pub struct TcpListener {
    inner: Arc<Mutex<StdTcpListener>>,
    reactor: Arc<Reactor>,
    pub(crate) id: SourceId,
}

impl TcpListener {
    pub fn id(&self) -> SourceId { self.id }

    /// Non-blocking accept. `Ok(Some(TcpStream))` on a fresh accept, `Ok(None)`
    /// on `WouldBlock`, or `Err` on real I/O errors.
    pub fn try_accept(&self) -> io::Result<Option<TcpStream>> {
        let std_l = self.inner.lock().unwrap();
        match std_l.accept() {
            Ok((stream, _addr)) => Ok(Some(stream)),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Blocking accept for Phase-3 sync caps. Toggles non-blocking = false
    /// briefly, performs accept, restores non-blocking = true.
    pub fn accept_blocking(&self) -> io::Result<TcpConnection> {
        let std_l = self.inner.lock().unwrap();
        std_l.set_nonblocking(false)?;
        let (stream, _addr) = std_l.accept()?;
        std_l.set_nonblocking(true)?;
        drop(std_l);
        stream.set_nonblocking(true)?;
        Ok(TcpConnection::wrap(stream, Arc::clone(&self.reactor)))
    }
}

impl Source for TcpListener {
    fn id(&self) -> SourceId { self.id }
    fn try_read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        // Listeners don't produce bytes; treat as idle.
        Ok(0)
    }
    fn try_write(&mut self, _: &[u8]) -> io::Result<usize> {
        Ok(0)
    }
    fn try_accept(&mut self) -> io::Result<Option<TcpStream>> {
        TcpListener::try_accept(self)
    }
}

impl AsyncAccept for TcpListener {
    type Conn = TcpConnection;
    fn poll_accept(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<TcpConnection>> {
        let me = self.get_mut();
        match me.try_accept() {
            Ok(Some(stream)) => {
                stream.set_nonblocking(true)?;
                Poll::Ready(Ok(TcpConnection::wrap(stream, Arc::clone(&me.reactor))))
            }
            Ok(None) => {
                // Register a self-reference via TcpListenerClone so the polling
                // thread wakes us when a connection arrives.
                me.reactor.register(
                    Box::new(TcpListenerClone {
                        id: me.id,
                        inner: Arc::clone(&me.inner),
                    }),
                    cx.waker().clone(),
                );
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e))
        }
    }
}

struct TcpListenerClone {
    id: SourceId,
    inner: Arc<Mutex<StdTcpListener>>,
}

impl Source for TcpListenerClone {
    fn id(&self) -> SourceId { self.id }
    fn try_read(&mut self, _: &mut [u8]) -> io::Result<usize> { Ok(0) }
    fn try_write(&mut self, _: &[u8]) -> io::Result<usize> { Ok(0) }
    fn try_accept(&mut self) -> io::Result<Option<TcpStream>> {
        let std_l = self.inner.lock().unwrap();
        match std_l.accept() {
            Ok((stream, _addr)) => Ok(Some(stream)),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }
}

fn next_source_id() -> SourceId {
    static COUNTER: AtomicUsize = AtomicUsize::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn tcp_transport_connect_smoke() {
        let reactor = Arc::new(Reactor::new());
        let tp = TcpTransport::new(reactor);
        let e = Endpoint(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let _ = tp.connect_endpoint(&e);
        let _ = tp.listen_endpoint(&e);
    }
}
