//! RustPython guest runtime for CrushVM — multi-lane Python execution.
//!
//! # Python Execution Lanes
//!
//! ```text
//! CrushVM Python Stack
//! ├── Lane 1: py→CAST→CASM       safest / native speed
//! ├── Lane 2: embedded CrushPy   dynamic pure Python, cap-gated
//! └── Lane 3: cpython subprocess full ecosystem fallback
//! ```
//!
//! ## Lane 2: Embedded CrushPy (RustPython)
//!
//! Embedded RustPython should not pretend to be normal Python.
//! It is **Python syntax inside Crush authority.**
//!
//! ### Execution Profiles
//!
//! | Profile | Builtins | Imports | Use Case |
//! |---------|----------|---------|---------|
//! | `crushpy-core` | Tiny (print, len, range, int, str...) | None | Pure computation |
//! | `crushpy-exo` | Core + exo bridge | exo.fs, exo.log, exo.env | Capability-gated I/O |
//! | `crushpy-compat` | Broader stdlib | Math, json, re, itertools | Richer scripts |
//! | `cpython-external` | Full CPython | Everything | numpy, pandas, torch |
//!
//! ### Import Firewall
//!
//! No normal imports by default. Only `exo.*` modules are allowed:
//! - `import exo.fs` → allowed
//! - `import os` → denied
//! - `import socket` → denied
//!
//! ### Frozen Builtins
//!
//! Dangerous builtins are replaced or capability-gated:
//! - `print` → ctx.stdout
//! - `open` → denied unless `io.open` capability granted
//! - `eval` / `exec` / `compile` → denied unless `sys.eval` cap granted
//! - `__import__` → capability-gated
//!
//! ### Fuel / Step Budget
//!
//! Every embedded script runs under limits:
//! - `max_steps` = 1_000_000
//! - `max_memory` = 64 MB
//! - `max_time_ms` = 500
//! - `max_recursion` = 128
//!
//! ### Value Bridge
//!
//! ```text
//! Crush    ↔ Python
//! null     ↔ None
//! bool     ↔ bool
//! int      ↔ int
//! float    ↔ float
//! str      ↔ str
//! list     ↔ list
//! map      ↔ dict
//! bytes    ↔ bytes
//! opaque   → OpaquePyObject(handle)
//! ```
//!
//! ### Error Mapping
//!
//! - Python `SyntaxError` → Crush `CompileError`
//! - Python `NameError` → Crush `RuntimeError`
//! - Python `PermissionError` → Crush `CapabilityDenied`
//! - Python `RecursionError` → Crush `LimitExceeded`
//! - Python timeout → Crush `FuelExhausted`

pub mod profile;
pub mod router;

#[cfg(feature = "rustpython")]
pub mod rustpython_backend;

use crush_runtime_abi::{GuestContext, GuestValue};

use router::{PythonBackend, PythonRouter};

/// Execute Python source code through the best available backend.
pub fn execute_python(source: &str, ctx: &GuestContext) -> anyhow::Result<GuestValue> {
    let router = PythonRouter::new();
    match router.choose_backend(source) {
        PythonBackend::CastTranspile => {
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
            anyhow::bail!("subprocess backend not yet wired from this entry point")
        }
    }
}
