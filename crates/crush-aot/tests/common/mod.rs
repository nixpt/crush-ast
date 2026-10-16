//! Shared by the corpus harnesses (`js_parity`, `python_parity`, `rust_parity`):
//! build and run the programs on a pool of workers instead of one after another.

use std::panic::{self, AssertUnwindSafe};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Parallel builds. Each is a rustc or gcc process of its own, next to the other test
/// binaries `cargo test` runs, so stay well under the core count by default. Same knob
/// as `examples_parity.rs`: `CRUSH_AOT_PARITY_JOBS`.
pub fn worker_count() -> usize {
    std::env::var("CRUSH_AOT_PARITY_JOBS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n: &usize| n > 0)
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()).min(4))
}

/// `items.iter().map(f).collect()`, with `f` run on [`worker_count`] threads.
///
/// Results come back in `items` order, so a harness that checks them in a plain loop
/// afterwards prints and reports exactly what its serial loop did. A panic in `f` is
/// re-raised here, with its original payload, once every item has run.
pub fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<std::thread::Result<R>>>> = items.iter().map(|_| Mutex::new(None)).collect();
    std::thread::scope(|scope| {
        for _ in 0..worker_count().min(items.len()) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(i) else { break };
                    let result = panic::catch_unwind(AssertUnwindSafe(|| f(item)));
                    *slots[i].lock().unwrap() = Some(result);
                }
            });
        }
    });
    slots
        .into_iter()
        .map(|slot| match slot.into_inner().unwrap().expect("every item ran") {
            Ok(r) => r,
            Err(payload) => panic::resume_unwind(payload),
        })
        .collect()
}
