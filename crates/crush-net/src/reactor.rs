//! Cross-platform, zero-dep polling reactor.
//!
//! Strategy: one background `std::thread` periodically walks an
//! `Arc<Mutex<HashMap<SourceId, Entry>>>` of registered sources, calling
//! `Source::try_accept` / `try_read` / `try_write` on each. On
//! `ErrorKind::WouldBlock` the source is left alone; on `Ready` the registered
//! `Waker` is fired. Between passes the poller sleeps `POLL_INTERVAL` (10 ms).
//!
//! Phase-3 addition: `Source::try_accept` is the waker hook for listeners. The
//! default implementation returns `WouldBlock` so non-acceptor sources compile
//! unchanged; `TcpListener` overrides it.
//!
//! Lifecycle: `spawn_poller` returns a [`PollerHandle`] whose `Drop` sets
//! `stop = true` and joins. `_ = reactor.spawn_poller()` is leak-free.

use std::{
    collections::HashMap,
    io,
    net::TcpStream,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    task::Waker,
    thread,
    time::Duration,
};

const POLL_INTERVAL: Duration = Duration::from_millis(10);

pub type SourceId = usize;

pub trait Source: Send + 'static {
    fn id(&self) -> SourceId;
    fn try_read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
    fn try_write(&mut self, buf: &[u8]) -> io::Result<usize>;
    /// Default: not an acceptor. `TcpListener` overrides.
    fn try_accept(&mut self) -> io::Result<Option<TcpStream>> {
        Err(io::Error::new(io::ErrorKind::WouldBlock, "not an acceptor"))
    }
}

pub struct Reactor {
    inner: Arc<Mutex<HashMap<SourceId, Entry>>>,
    stop: Arc<AtomicBool>,
}

struct Entry {
    source: Box<dyn Source>,
    waker: Waker,
}

impl Reactor {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn register(&self, source: Box<dyn Source>, waker: Waker) {
        let id = source.id();
        self.inner.lock().unwrap().insert(id, Entry { source, waker });
    }

    pub fn unregister(&self, id: SourceId) {
        self.inner.lock().unwrap().remove(&id);
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn spawn_poller(self: &Arc<Self>) -> PollerHandle {
        let inner = Arc::clone(&self.inner);
        let stop = Arc::clone(&self.stop);
        let join = thread::Builder::new()
            .name("crush-net-poller".into())
            .spawn(move || run_poller(inner, stop))
            .expect("failed to spawn reactor poller thread");
        PollerHandle::new(join, Arc::clone(&self.stop))
    }
}

impl Default for Reactor {
    fn default() -> Self { Self::new() }
}

pub struct PollerHandle {
    join: Option<thread::JoinHandle<()>>,
    stop: Arc<AtomicBool>,
}

impl PollerHandle {
    fn new(join: thread::JoinHandle<()>, stop: Arc<AtomicBool>) -> Self {
        Self { join: Some(join), stop }
    }

    pub fn shutdown(mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

impl Drop for PollerHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

fn run_poller(inner: Arc<Mutex<HashMap<SourceId, Entry>>>, stop: Arc<AtomicBool>) {
    let mut scratch = vec![0u8; 4096];
    while !stop.load(Ordering::Acquire) {
        let mut to_wake: Vec<Waker> = Vec::new();
        let mut dead: Vec<SourceId> = Vec::new();
        {
            let mut guard = inner.lock().unwrap();
            for (id, entry) in guard.iter_mut() {
                match entry.source.try_accept() {
                    Ok(Some(_)) => to_wake.push(entry.waker.clone()),
                    Ok(None) => {}
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Err(_) => to_wake.push(entry.waker.clone()),
                }
                match entry.source.try_read(&mut scratch) {
                    Ok(0) => {
                        to_wake.push(entry.waker.clone());
                        dead.push(*id);
                        continue;
                    }
                    Ok(_) => to_wake.push(entry.waker.clone()),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Err(_) => {
                        to_wake.push(entry.waker.clone());
                        dead.push(*id);
                    }
                }
                match entry.source.try_write(&[]) {
                    Ok(_) => {}
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Err(_) => to_wake.push(entry.waker.clone()),
                }
            }
            for id in &dead {
                guard.remove(id);
            }
        }
        for w in to_wake {
            w.wake();
        }
        thread::sleep(POLL_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::{RawWaker, RawWakerVTable, Waker};

    fn noop_waker() -> Waker {
        static VT: RawWakerVTable = RawWakerVTable::new(
            |_| RawWaker::new(std::ptr::null(), &VT),
            |_| {},
            |_| {},
            |_| {},
        );
        unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VT)) }
    }

    struct DummySource { id: SourceId }
    impl Source for DummySource {
        fn id(&self) -> SourceId { self.id }
        fn try_read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::WouldBlock, "noop"))
        }
        fn try_write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::WouldBlock, "noop"))
        }
    }

    #[test]
    fn register_increments_len() {
        let r = Reactor::new();
        r.register(Box::new(DummySource { id: 42 }), noop_waker());
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn poller_stops_when_handle_drops() {
        let reactor = Arc::new(Reactor::new());
        let handle = reactor.spawn_poller();
        thread::sleep(POLL_INTERVAL * 3);
        drop(handle);
    }
}
