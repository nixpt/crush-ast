//! Python bindings for crush-cast — parse, validate, and inspect CAST IR.
//!
//! Build with:
//!   cargo build -p crush-python
//!   maturin develop  (or `maturin build` for a wheel)

use pyo3::prelude::*;

/// Parse a CAST JSON string into a validated Program and return its JSON repr.
#[pyfunction]
fn parse_cast(json: &str) -> PyResult<String> {
    let program: crush_cast::Program = serde_json::from_str(json)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    serde_json::to_string_pretty(&program)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}

/// Validate a CAST JSON string. Returns True if valid, raises ValueError otherwise.
#[pyfunction]
fn validate_cast(json: &str) -> PyResult<bool> {
    let program: crush_cast::Program = serde_json::from_str(json)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    let _ = program;
    Ok(true)
}

/// List all CAST version strings known to this library.
#[pyfunction]
fn cast_version() -> &'static str {
    "0.2"
}

/// Python module: crush
#[pymodule]
fn crush(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse_cast, m)?)?;
    m.add_function(wrap_pyfunction!(validate_cast, m)?)?;
    m.add_function(wrap_pyfunction!(cast_version, m)?)?;
    Ok(())
}
