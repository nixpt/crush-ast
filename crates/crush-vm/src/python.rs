#![allow(unsafe_op_in_unsafe_fn)]
//! PyO3 bindings (CRUSHVM-PYO3): expose the canonical CVM1 VM to Python so
//! chroma/tessera delegates execution to the SAME Rust VM that crush-ast +
//! crush-ptx use — CVM1 round-trip becomes correct-by-construction.
//! Feature-gated behind `python`; built as a cdylib via maturin.
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::bytecode::Program;
use crate::vm::{run, Quotas, Value};

fn value_to_py(py: Python<'_>, v: &Value) -> PyObject {
    match v {
        Value::Null => py.None(),
        Value::Bool(b) => b.into_py(py),
        Value::Int(i) => i.into_py(py),
        Value::Float(f) => f.into_py(py),
        Value::Str(s) => s.into_py(py),
        other => format!("{other:?}").into_py(py),
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
#[pyfunction]
#[pyo3(signature = (blob, max_steps=None, max_output=None, max_stack=None, max_call_depth=None))]
fn run_blob<'py>(
    py: Python<'py>,
    blob: &[u8],
    max_steps: Option<usize>,
    max_output: Option<usize>,
    max_stack: Option<usize>,
    max_call_depth: Option<usize>,
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
    let res = run(&program, &q).map_err(|e| PyRuntimeError::new_err(format!("CVM1 trap: {e:?}")))?;
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
