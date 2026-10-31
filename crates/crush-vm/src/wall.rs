//! Wall-clock budget shared by the scheduler and `PortableVm` (CRUSH-186).
//!
//! `Quotas::max_wall_time_ms` used to be handed whole to every blocking
//! capability, so a program could chain `limit`-sized calls, and nothing
//! looked at the clock between instructions, so a plain `while true` with a
//! large `max_steps` ran for as long as it liked. The run now has one
//! deadline: the step loops check it every [`CHECK_EVERY`] instructions, and
//! blocking capabilities are given what is *left* of it.

use std::cell::Cell;
use std::time::{Duration, Instant};

use crate::vm::VmError;

/// Instructions between clock reads: `Instant::now()` is cheap but not free.
pub const CHECK_EVERY: usize = 1024;

thread_local! {
    /// The active run's deadline and its configured limit (for error text).
    static DEADLINE: Cell<Option<(Instant, u64)>> = const { Cell::new(None) };
}

/// Restores the previous deadline when dropped.
pub struct DeadlineGuard(Option<(Instant, u64)>);

impl Drop for DeadlineGuard {
    fn drop(&mut self) {
        DEADLINE.with(|d| d.set(self.0));
    }
}

/// Make `deadline` (from a `limit_ms` budget) the current run's deadline on
/// this thread. An already installed earlier deadline (an enclosing run) wins.
pub fn enter(deadline: Instant, limit_ms: u64) -> DeadlineGuard {
    let prev = DEADLINE.with(|d| d.get());
    let next = match prev {
        Some((p, _)) if p <= deadline => prev,
        _ => Some((deadline, limit_ms)),
    };
    DEADLINE.with(|d| d.set(next));
    DeadlineGuard(prev)
}

/// The configured limit of the active run, if any (what an error should name).
pub fn limit_ms() -> Option<u64> {
    DEADLINE.with(|d| d.get()).map(|(_, l)| l)
}

/// Time left until the current run's deadline, if a run is active on this
/// thread. Blocking capabilities (`process.exec`) use this as their timeout.
pub fn remaining() -> Option<Duration> {
    DEADLINE.with(|d| d.get()).map(|(d, _)| d.saturating_duration_since(Instant::now()))
}

/// The budget to hand a blocking capability: what is left of the run's
/// deadline, or `limit_ms` when no run is active. Never 0 (0 reads as
/// "no time at all" to some callers); the step loop reports the real expiry.
pub fn remaining_ms(limit_ms: u64) -> u64 {
    remaining().map_or(limit_ms, |r| (r.as_millis() as u64).max(1))
}

/// Error for a run that outlived `Quotas::max_wall_time_ms`.
pub fn expired(limit_ms: u64) -> VmError {
    VmError::WallTimeQuota(limit_ms)
}

/// `Err(WallTimeQuota)` when `deadline` has passed.
pub fn check(deadline: Instant, limit_ms: u64) -> Result<(), VmError> {
    if Instant::now() >= deadline {
        return Err(expired(limit_ms));
    }
    Ok(())
}
