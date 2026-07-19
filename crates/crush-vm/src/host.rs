//! Host capability extension point for crush-vm.
//!
//! The built-in capability registry in [`crate::caps`] only contains portable
//! operations (I/O and string ops). Hosts that embed crush-vm can register
//! additional capabilities here without forking the VM.

use std::collections::HashMap;

use crate::vm::Value;

/// Metadata describing a host-provided capability.
#[derive(Debug, Clone)]
pub struct HostCapSpec {
    pub name: String,
    /// Fixed argument count, or `None` if variadic.
    pub argc: Option<usize>,
    /// Does the capability push a result onto the stack?
    pub returns: bool,
}

/// Error from [`HostCap::call_with_deadline`] — distinguishes "self-enforced
/// wall-clock timeout" from an ordinary capability failure so `dispatch_cap`
/// can map it to `VmError::CapTimeout` instead of `VmError::CapDenied`.
pub enum HostCapError {
    /// An ordinary failure message (same shape `call()` has always returned).
    Message(String),
    /// The capability self-enforced `deadline_ms` from
    /// [`HostCap::call_with_deadline`] and gave up before completing.
    Timeout,
}

impl From<String> for HostCapError {
    fn from(msg: String) -> Self {
        HostCapError::Message(msg)
    }
}

/// Trait for host-provided capabilities.
pub trait HostCap: Send + Sync {
    /// Return this capability's metadata.
    fn spec(&self) -> HostCapSpec;

    /// Execute the capability with the given arguments.
    ///
    /// Returns `Ok(Some(value))` to push a result, `Ok(None)` for no result,
    /// or `Err(message)` to raise a VM error.
    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String>;

    /// Like `call`, but the capability is told the wall-clock budget
    /// (milliseconds, from `Quotas::max_wall_time_ms`) it should self-enforce
    /// if it can legitimately block (network I/O, cold resource
    /// provisioning, ...).
    ///
    /// `CAP_CALL`'s generic dispatch cannot preempt an arbitrary
    /// `HostCap::call()` from the *outside*: `Value` carries
    /// `Rc<RefCell<...>>` fields that aren't `Send`, so the call can't
    /// safely be moved onto a watchdog thread and killed on timeout the way
    /// `EXEC_LANG`'s OS subprocess can (see
    /// `scheduler::run_with_wall_clock_limit`). Bounding is therefore
    /// cooperative: a `HostCap` that can block overrides this method and
    /// enforces `deadline_ms` internally (e.g. a bucket-provisioning cap
    /// wrapping a subprocess reuses `run_with_wall_clock_limit`'s own
    /// poll-against-a-deadline shape). The default delegates straight to
    /// `call()` — existing capabilities that can't block need no change.
    fn call_with_deadline(
        &self,
        args: Vec<Value>,
        deadline_ms: u64,
    ) -> Result<Option<Value>, HostCapError> {
        let _ = deadline_ms;
        self.call(args).map_err(HostCapError::Message)
    }
}

/// Registry of host-provided capabilities.
#[derive(Default)]
pub struct HostCaps {
    handlers: HashMap<String, Box<dyn HostCap>>,
}

impl HostCaps {
    /// Create an empty host-capability registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a capability handler.
    /// Grant polyglot execution for the given canonical languages ("python"/"javascript"/"bash").
    /// Registers a `polyglot.<lang>` gate whose PRESENCE authorizes exec_lang to spawn that
    /// interpreter. This is the crush-vm-native grant; higher layers (the sdk builder, exo-light's
    /// Enforcer) funnel through it. Off by default — polyglot is not ambient.
    pub fn grant_polyglot(&mut self, langs: &[&str]) -> &mut Self {
        for &lang in langs {
            self.register(polyglot_gate(lang));
        }
        self
    }

    pub fn register(&mut self, handler: Box<dyn HostCap>) -> &mut Self {
        let name = handler.spec().name.clone();
        self.handlers.insert(name, handler);
        self
    }

    /// Look up a handler by capability name.
    pub fn get(&self, name: &str) -> Option<&dyn HostCap> {
        self.handlers.get(name).map(|b| b.as_ref())
    }

    /// Return the names of all registered capabilities.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.handlers.keys().map(|s| s.as_str())
    }

    /// Return a cloned spec for a registered capability, if any.
    pub fn spec(&self, name: &str) -> Option<HostCapSpec> {
        self.handlers.get(name).map(|h| h.spec())
    }
}

impl std::fmt::Debug for HostCaps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostCaps")
            .field("caps", &self.handlers.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// Construct a single polyglot gate handler for one canonical language ("python"/"javascript"/
/// "bash"). This is the seam higher layers plug into: exo-light's `Enforcer::derive` turns a
/// `Named("polyglot.<lang>")` capability from a CapabilitySet into `polyglot_gate("<lang>")` and
/// pushes it, so a capsule's declared polyglot grant becomes a live gate with no crush-vm change.
pub fn polyglot_gate(lang: &str) -> Box<dyn HostCap> {
    Box::new(PolyglotGate { lang: lang.to_string() })
}

/// Presence-only capability gate for `@<lang>` polyglot blocks. exec_lang checks
/// `host_caps.get("polyglot.<lang>")` before spawning; this handler's mere registration is the
/// authorization. `call()` is never reached through normal execution.
struct PolyglotGate {
    lang: String,
}

impl HostCap for PolyglotGate {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: format!("polyglot.{}", self.lang), argc: None, returns: false }
    }
    fn call(&self, _args: Vec<crate::vm::Value>) -> Result<Option<crate::vm::Value>, String> {
        Ok(None)
    }
}
