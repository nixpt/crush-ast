//! RustPython guest runtime for CrushVM — multi-lane Python execution.
//!
//! # Python Execution Lanes
//!
//! Crush can run Python code through three backends, selected automatically
//! by [`PythonRouter`]:
//!
//! | Lane | Backend | Best for | Speed | Security |
//! |------|---------|----------|-------|----------|
//! | 1 | CAST/CASM transpile | Simple subset (`x = 1; print(x)`) | ⚡ Native VM | ✅ Full |
//! | 2 | RustPython embedded | Dynamic pure Python (classes, methods) | 🏃 In-process | ✅ Cap-gated |
//! | 3 | Subprocess | CPython ecosystem (numpy, pandas) | 🐢 External | ⚠️ Process-level |
//!
//! When the `rustpython` feature is enabled, the RustPython VM backend
//! is available for lane 2. Without it, only lanes 1 and 3 are usable.

pub mod router;

#[cfg(feature = "rustpython")]
pub mod rustpython_backend;

use crush_runtime_abi::{GuestContext, GuestValue};

use router::{PythonBackend, PythonRouter};

/// Execute Python source code through the best available backend.
///
/// The router selects the lane based on code analysis:
/// 1. Simple code → CAST transpile (no runtime needed)
/// 2. Dynamic Python → RustPython (requires `rustpython` feature)
/// 3. C-extension imports → Subprocess (external Python)
pub fn execute_python(source: &str, ctx: &GuestContext) -> anyhow::Result<GuestValue> {
    let router = PythonRouter::new();
    match router.choose_backend(source) {
        PythonBackend::CastTranspile => {
            // Uses the existing py_walker + crush_frontend pipeline
            anyhow::bail!("CAST transpile not yet wired from this entry point")
        }
        PythonBackend::RustPythonEmbedded => {
            #[cfg(feature = "rustpython")]
            {
                let mut rt = rustpython_backend::RustPythonBackend::new();
                rt.eval_source(source, ctx)
            }
            #[cfg(not(feature = "rustpython"))]
            {
                let _ = (source, ctx);
                anyhow::bail!(
                    "RustPython backend not available (compile with --features rustpython)"
                )
            }
        }
        PythonBackend::Subprocess => {
            // Uses the existing @python { } subprocess dispatch
            anyhow::bail!("subprocess backend not yet wired from this entry point")
        }
    }
}
