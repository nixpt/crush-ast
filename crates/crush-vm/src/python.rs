#![allow(unsafe_op_in_unsafe_fn)]
//! PyO3 bindings (CRUSHVM-PYO3): expose the canonical CVM1 VM to Python so
//! chroma/tessera delegates execution to the SAME Rust VM that crush-ast +
//! crush-ptx use — CVM1 round-trip becomes correct-by-construction.
//! Feature-gated behind `python`; built as a cdylib via maturin.
//!
//! CRUSHVM-CAPS-2 (Phase 2): `run_blob` additionally accepts `host_caps` (a
//! `{name: (callable, argc_or_None, returns)}` dict) and `allowed_caps` (a
//! restriction list forwarded straight onto `Quotas::allowed_caps`), so a
//! Python embedder can bridge its own host-provided capabilities (chroma's
//! `frame.emit`/`bus.send`/`bus.recv`) through to the wheel instead of
//! falling back to the pure-Python VM whenever those caps are declared. Both
//! kwargs default to `None` — existing callers are unaffected.
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyTuple};

use crate::bytecode::Program;
use crate::host::{HostCap, HostCapSpec, HostCaps};
use crate::vm::{run_with_caps, Quotas, Value};

fn value_to_py(py: Python<'_>, v: &Value) -> PyObject {
    match v {
        Value::Null => py.None(),
        Value::Bool(b) => b.into_py(py),
        Value::Int(i) => i.into_py(py),
        Value::Float(f) => f.into_py(py),
        Value::Str(s) => s.into_py(py),
        // Binary payloads (e.g. a `frame.emit` argument built from
        // `bytes(...)` on the chroma side) round-trip as a real Python
        // `bytes` object -- NOT `str` (a lossy/wrong mapping for arbitrary
        // binary data) and not the Rust `Debug` fallback below.
        Value::Bytes(b) => PyBytes::new_bound(py, b).into_py(py),
        other => format!("{other:?}").into_py(py),
    }
}

/// Inverse of [`value_to_py`] — converts a Python object returned by a
/// bridged `HostCap` closure back into a `Value`. Supports the same set
/// `value_to_py` produces (`None`/`bool`/`int`/`float`/`str`/`bytes`); any
/// other Python type is a host-cap authoring error, not a VM fault, so it
/// comes back as a plain `Err(String)` (mapped to `VmError::UnknownCap` by
/// the caller, same as any other host-cap failure message).
///
/// Ordering note: Python `bool` is a subtype of `int` at the C level, so
/// `PyLong_AsLongLong` happily converts `True`/`False` to `1`/`0` — the
/// `bool` check MUST run before the `i64` check, or every bridged `bool`
/// return value would silently become `Value::Int`.
fn py_to_value(obj: &Bound<'_, PyAny>) -> Result<Value, String> {
    if obj.is_none() {
        return Ok(Value::Null);
    }
    if let Ok(b) = obj.extract::<bool>() {
        return Ok(Value::Bool(b));
    }
    if let Ok(i) = obj.extract::<i64>() {
        return Ok(Value::Int(i));
    }
    if let Ok(f) = obj.extract::<f64>() {
        return Ok(Value::Float(f));
    }
    if let Ok(s) = obj.extract::<String>() {
        return Ok(Value::Str(s));
    }
    if let Ok(b) = obj.extract::<Vec<u8>>() {
        return Ok(Value::Bytes(b));
    }
    let type_name = obj
        .get_type()
        .name()
        .map(|n| n.to_string())
        .unwrap_or_else(|_| "<unknown>".to_string());
    Err(format!(
        "host cap returned unsupported Python type {type_name:?} (expected \
         None/bool/int/float/str/bytes)"
    ))
}

/// Bridges a Python callable into crush-vm's [`HostCap`] trait. `callable`
/// is a `Py<PyAny>` handle — `Send + Sync` (a refcounted pointer into the
/// interpreter, not the object itself) — so `PyHostCap` satisfies `HostCap:
/// Send + Sync` with no unsafe code. `call()`'s trait signature carries no
/// `Python<'_>` token (crush-vm is not pyo3-aware), so the GIL is
/// reacquired inside via `Python::with_gil`; this is safe REENTRANT
/// acquisition, not a deadlock risk — `run_blob` already holds the GIL on
/// the calling thread for the whole `run_with_caps` call, and
/// `Python::with_gil` is documented as safe to call on a thread that
/// already holds it.
struct PyHostCap {
    name: String,
    argc: Option<usize>,
    returns: bool,
    callable: Py<PyAny>,
}

impl HostCap for PyHostCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: self.name.clone(), argc: self.argc, returns: self.returns }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        Python::with_gil(|py| {
            let py_args: Vec<PyObject> = args.iter().map(|v| value_to_py(py, v)).collect();
            let tuple = PyTuple::new_bound(py, &py_args);
            let result = self
                .callable
                .bind(py)
                .call1(tuple)
                .map_err(|e| format!("{}: {e}", self.name))?;
            if self.returns {
                py_to_value(&result).map(Some)
            } else {
                Ok(None)
            }
        })
    }
}

/// Load + run a CVM1 blob (the shared `.cvm1` binary format). Returns a dict
/// {output, result, stack, steps, halted}. `result` = top of the value stack
/// (a function's return value is left there), so e.g. `fib(10)` → result 55.
/// This is the reference VM — a program from any crush toolchain runs identically.
///
/// Quotas: `max_steps`/`max_output`/`max_stack`/`max_call_depth` map 1:1 onto
/// `Quotas` (unset = `Quotas::default()` for that field). `max_frames` is a
/// python-VM-only visual-frames concept (chroma's `vm.py`) with no Rust
/// counterpart — it is NOT accepted here and stays enforced only by the
/// python fallback path.
///
/// `allowed_caps`, if given, further restricts the program's declared
/// permissions (maps 1:1 onto `Quotas::allowed_caps`) — matched by EXACT
/// NAME against the CAP_CALL's base capability name, same as crush-vm's
/// native `dispatch_cap`. It does NOT understand topic-scoped permission
/// tokens (chroma's `"bus.send:chat"` convention, chromacapsule.md §4) —
/// a caller with scoped-only grants must not pass them here (see chroma's
/// `run()` routing guard, which keeps such programs on the Python VM).
///
/// `host_caps`, if given, is a `{name: (callable, argc_or_None, returns)}`
/// dict (CRUSHVM-CAPS-2): each entry registers a [`PyHostCap`] so a
/// `CAP_CALL` the program declares but which isn't in crush-vm's built-in
/// portable registry (chroma's `frame.emit`/`bus.send`/`bus.recv`) can be
/// served by a Python closure instead of trapping `UnknownCap`. `argc`
/// (`None` = variadic) and `returns` are enforced the same way as any other
/// capability's spec — arity mismatches raise `CapArity` before the
/// callable is ever invoked. A callable's own Python exception surfaces as
/// `VmError::UnknownCap("<cap>: <message>")` (the existing host-cap failure
/// path — `HostCap::call`'s error channel is a flat `String`, so there is
/// no dedicated Rust trap variant for "the closure's own business-logic
/// check failed"; this is the same shape any other `HostCap`'s failure
/// takes, not something new introduced here).
#[pyfunction]
#[pyo3(signature = (blob, max_steps=None, max_output=None, max_stack=None, max_call_depth=None, host_caps=None, allowed_caps=None))]
fn run_blob<'py>(
    py: Python<'py>,
    blob: &[u8],
    max_steps: Option<usize>,
    max_output: Option<usize>,
    max_stack: Option<usize>,
    max_call_depth: Option<usize>,
    host_caps: Option<Bound<'py, PyDict>>,
    allowed_caps: Option<Vec<String>>,
) -> PyResult<Bound<'py, PyDict>> {
    let program = Program::from_blob(blob)
        .map_err(|e| PyValueError::new_err(format!("CVM1 load error: {e:?}")))?;
    let mut q = Quotas::default();
    if let Some(ms) = max_steps {
        q.max_steps = ms;
    }
    if let Some(mo) = max_output {
        q.max_output = mo;
    }
    if let Some(ms) = max_stack {
        q.max_stack = ms;
    }
    if let Some(mcd) = max_call_depth {
        q.max_call_depth = mcd;
    }
    q.allowed_caps = allowed_caps;

    let mut hc = HostCaps::new();
    if let Some(dict) = &host_caps {
        for (k, v) in dict.iter() {
            let name: String = k.extract().map_err(|_| {
                PyValueError::new_err("host_caps keys must be str")
            })?;
            let (callable, argc, returns): (Py<PyAny>, Option<usize>, bool) =
                v.extract().map_err(|e: PyErr| {
                    PyValueError::new_err(format!(
                        "host_caps[{name:?}] must be (callable, argc_or_None, returns): {e}"
                    ))
                })?;
            hc.register(Box::new(PyHostCap { name, argc, returns, callable }));
        }
    }

    // Always dispatch through `run_with_caps`, even when `hc` is empty: an
    // empty registry is behaviourally identical to `run()`'s `None` (the
    // builtin-registry match arms are checked first regardless, and an
    // empty `HashMap::get` misses exactly like `Option::None` does) — one
    // code path instead of branching on `host_caps.is_some()`.
    let res = run_with_caps(&program, &q, Some(&hc))
        .map_err(|e| PyRuntimeError::new_err(format!("CVM1 trap: {e:?}")))?;
    let stack: Vec<PyObject> = res.stack.iter().map(|v| value_to_py(py, v)).collect();
    let result = res.stack.last().map(|v| value_to_py(py, v));
    let d = PyDict::new_bound(py);
    d.set_item("output", res.output)?;
    d.set_item("result", result)?;
    d.set_item("stack", stack)?;
    d.set_item("steps", res.steps)?;
    d.set_item("halted", res.halted)?;
    Ok(d)
}

#[pymodule]
fn crush_vm(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(run_blob, m)?)?;
    Ok(())
}
