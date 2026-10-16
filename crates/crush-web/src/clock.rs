//! `time.*` for a browser run granted `time` (`crush-run --time`).
//!
//! crush-lang-sdk's clock capabilities read `std::time::SystemTime`, which
//! panics on `wasm32-unknown-unknown`, so a browser run gets these instead:
//! the same names, arguments, results and errors, with the clock read from
//! JavaScript's `Date.now()` (milliseconds). `time.now_iso` is therefore
//! precise to the millisecond (`…T12:00:00.123+00:00`) where a native run
//! usually prints nanoseconds. `time.sleep` / `async.sleep` wait by spinning,
//! because a browser worker cannot block; the wait counts as running time.
//!
//! On other targets (crush-web's native tests) the clock is the system clock.

use crush_vm::host::HostCapError;
use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};

#[cfg(target_arch = "wasm32")]
fn now_ms_f64() -> f64 {
    js_sys::Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
fn now_ms_f64() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

fn now_ms() -> i64 {
    now_ms_f64().floor() as i64
}

/// Register the clock family: `time.now`, `time.now_ms`, `time.now_iso`,
/// `time.elapsed`, `time.sleep` and `async.sleep`.
pub fn register(caps: &mut HostCaps) {
    caps.register(Box::new(Clock::Now));
    caps.register(Box::new(Clock::NowMs));
    caps.register(Box::new(Clock::NowIso));
    caps.register(Box::new(Clock::Elapsed));
    caps.register(Box::new(Clock::Sleep("time.sleep")));
    caps.register(Box::new(Clock::Sleep("async.sleep")));
}

enum Clock {
    Now,
    NowMs,
    NowIso,
    Elapsed,
    Sleep(&'static str),
}

impl HostCap for Clock {
    fn spec(&self) -> HostCapSpec {
        let (name, argc) = match self {
            Clock::Now => ("time.now", 0),
            Clock::NowMs => ("time.now_ms", 0),
            Clock::NowIso => ("time.now_iso", 0),
            Clock::Elapsed => ("time.elapsed", 1),
            Clock::Sleep(name) => (*name, 1),
        };
        HostCapSpec {
            name: name.to_string(),
            argc: Some(argc),
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        match self {
            Clock::Now => Ok(Some(Value::Int(now_ms().div_euclid(1000)))),
            Clock::NowMs => Ok(Some(Value::Int(now_ms()))),
            Clock::NowIso => {
                let now = chrono::DateTime::from_timestamp_millis(now_ms())
                    .ok_or_else(|| "time.now_iso: clock out of range".to_string())?;
                Ok(Some(Value::Str(now.to_rfc3339())))
            }
            Clock::Elapsed => match &args[0] {
                Value::Int(start) => Ok(Some(Value::Int(now_ms() - start))),
                other => Err(format!(
                    "time.elapsed: expected int milliseconds, got {other}"
                )),
            },
            Clock::Sleep(name) => sleep_ms(name, &args, None).map_err(|e| match e {
                HostCapError::Message(m) => m,
                HostCapError::Timeout => format!("{name}: timed out"),
                HostCapError::Exit(code) => format!("{name}: exit {code}"),
            }),
        }
    }

    fn call_with_deadline(
        &self,
        args: Vec<Value>,
        deadline_ms: u64,
    ) -> Result<Option<Value>, HostCapError> {
        match self {
            Clock::Sleep(name) => sleep_ms(name, &args, Some(deadline_ms)),
            _ => self.call(args).map_err(HostCapError::Message),
        }
    }
}

/// crush-lang-sdk's `sleep_ms`: wait `ms` milliseconds, or until the VM's
/// wall-clock deadline when that comes first, and then report a timeout.
fn sleep_ms(
    cap: &str,
    args: &[Value],
    deadline_ms: Option<u64>,
) -> Result<Option<Value>, HostCapError> {
    let ms = match args.first() {
        Some(Value::Int(ms)) if *ms >= 0 => *ms as u64,
        other => {
            return Err(format!(
                "{cap}: expected non-negative int milliseconds, got {}",
                other.map_or("nothing".to_string(), |v| v.to_string())
            )
            .into());
        }
    };
    let timed_out = deadline_ms.is_some_and(|deadline| ms > deadline);
    wait(if timed_out {
        deadline_ms.unwrap_or(ms)
    } else {
        ms
    });
    if timed_out {
        return Err(HostCapError::Timeout);
    }
    Ok(Some(Value::Null))
}

#[cfg(target_arch = "wasm32")]
fn wait(ms: u64) {
    let end = now_ms_f64() + ms as f64;
    while now_ms_f64() < end {
        std::hint::spin_loop();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn wait(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms));
}
