//! RustPython embedded backend (lane 2).
//!
//! Uses the RustPython VM to execute Python code in-process.
//! Requires the `rustpython` feature.

use crush_runtime_abi::{GuestContext, GuestRuntime, GuestValue};

/// RustPython-based execution backend.
pub struct RustPythonBackend {
    #[allow(dead_code)]
    inner: Option<crush_runtime_abi::GuestValue>,
}

impl RustPythonBackend {
    pub fn new() -> Self {
        Self { inner: None }
    }

    pub fn eval_source(&mut self, source: &str, ctx: &GuestContext) -> anyhow::Result<GuestValue> {
        let _ = (source, ctx);
        anyhow::bail!("RustPython backend not yet implemented (compile with rustpython-vm dep)")
    }
}

impl GuestRuntime for RustPythonBackend {
    fn eval_source(&mut self, source: &str, ctx: &GuestContext) -> anyhow::Result<GuestValue> {
        self.eval_source(source, ctx)
    }

    fn call(
        &mut self,
        name: &str,
        args: &[GuestValue],
        ctx: &GuestContext,
    ) -> anyhow::Result<GuestValue> {
        let _ = (name, args, ctx);
        anyhow::bail!("RustPython call not yet implemented")
    }
}
