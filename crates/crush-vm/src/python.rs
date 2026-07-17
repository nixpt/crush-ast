#![allow(unsafe_op_in_unsafe_fn)]
//! PyO3 bindings (CRUSHVM-PYO3): expose the canonical CVM1 VM to Python so
//! chroma/tessera delegates execution to the SAME Rust VM that crush-ast +
//! crush-ptx use — CVM1 round-trip becomes correct-by-construction.
//! Feature-gated behind `python`; built as a cdylib via maturin.
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::bytecode::Program;
use crate::vm::{run, Quotas};

/// Load + run a CVM1 blob (the shared `.cvm1` binary format).
/// Returns a dict {output, steps, halted}. This is the reference VM — a program
/// produced by any crush toolchain runs here identically (correct-by-construction).
#[pyfunction]
#[pyo3(signature = (blob, max_steps=None))]
fn run_blob<'py>(
    py: Python<'py>,
    blob: &[u8],
    max_steps: Option<usize>,
) -> PyResult<Bound<'py, PyDict>> {
    let program = Program::from_blob(blob)
        .map_err(|e| PyValueError::new_err(format!("CVM1 load error: {e:?}")))?;
    let mut q = Quotas::default();
    if let Some(ms) = max_steps {
        q.max_steps = ms;
    }
    let res = run(&program, &q).map_err(|e| PyRuntimeError::new_err(format!("CVM1 trap: {e:?}")))?;
    let d = PyDict::new_bound(py);
    d.set_item("output", res.output)?;
    d.set_item("steps", res.steps)?;
    d.set_item("halted", res.halted)?;
    Ok(d)
}

#[pymodule]
fn crush_vm(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(run_blob, m)?)?;
    Ok(())
}
