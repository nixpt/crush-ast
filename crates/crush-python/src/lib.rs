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

/// Validate a CAST JSON string. Returns True when it parses as a CAST program
/// and its `cast_version` major is compatible with `cast_version()`; returns
/// False otherwise. Never raises for bad input: this is a verdict, not a parse.
#[pyfunction]
fn validate_cast(json: &str) -> bool {
    match serde_json::from_str::<crush_cast::Program>(json) {
        Ok(program) => program.check_version().is_ok(),
        Err(_) => false,
    }
}

/// Run a CASM JSON string using the FastVM.
///
/// Invalid CASM raises `ValueError`; a runtime fault (`FastYield::Error`, or a
/// load/lowering failure) raises `RuntimeError`. Only a completed run returns
/// (its yield state, as a debug string).
#[pyfunction]
fn run_casm(json: &str) -> PyResult<String> {
    let program: casm::Program = serde_json::from_str(json).map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!("Invalid CASM JSON: {}", e))
    })?;

    let yield_state = crush_vm::vm::run_fastvm(&program).map_err(|e| {
        pyo3::exceptions::PyRuntimeError::new_err(format!("VM Execution Error: {:?}", e))
    })?;

    if let crush_vm::fastvm::FastYield::Error(e) = &yield_state {
        return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
            "VM Execution Error: {:?}",
            e
        )));
    }
    Ok(format!("{:?}", yield_state))
}

/// Parse a CSON string.
#[pyfunction]
fn parse_cson(cson_str: &str) -> PyResult<String> {
    let mut parser = crush_caison::parser::CaisonParser::new(cson_str);
    let doc = parser
        .parse()
        .map_err(pyo3::exceptions::PyValueError::new_err)?;

    // For now, return a basic repr of the version and root to Python
    Ok(format!(
        "CaisonDocument(version={:?}, root={:?})",
        doc.version, doc.root
    ))
}

/// The CAST IR version this library writes and accepts (major-gated).
#[pyfunction]
fn cast_version() -> &'static str {
    crush_cast::CAST_VERSION
}

/// Python module `crush` (import name; must match `module-name` in
/// pyproject.toml, which sets the PyInit_ symbol maturin expects).
#[pymodule]
fn crush(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse_cast, m)?)?;
    m.add_function(wrap_pyfunction!(validate_cast, m)?)?;
    m.add_function(wrap_pyfunction!(run_casm, m)?)?;
    m.add_function(wrap_pyfunction!(parse_cson, m)?)?;
    m.add_function(wrap_pyfunction!(cast_version, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_CAST: &str = r#"{"cast_version":"0.1.0","entry":"main","functions":{}}"#;

    #[test]
    fn test_cast_version() {
        assert_eq!(cast_version(), crush_cast::CAST_VERSION);
    }

    #[test]
    fn test_parse_cast_valid() {
        let result = parse_cast(VALID_CAST);
        assert!(result.is_ok());
        let val = result.unwrap();
        assert!(val.contains("0.1.0"));
    }

    #[test]
    fn test_parse_cast_invalid() {
        let json = r#"{"invalid_field":true}"#;
        let result = parse_cast(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_cast_valid() {
        assert!(validate_cast(VALID_CAST));
    }

    #[test]
    fn test_validate_cast_invalid_returns_false() {
        assert!(!validate_cast(r#"{"invalid_field":true}"#));
        assert!(!validate_cast("not json at all"));
    }

    #[test]
    fn test_validate_cast_incompatible_major_returns_false() {
        let json = r#"{"cast_version":"9.0.0","entry":"main","functions":{}}"#;
        assert!(!validate_cast(json));
    }

    #[test]
    fn test_run_casm_runtime_fault_raises() {
        // Stack underflow: `add` with an empty stack. Must be an error, not a
        // debug string (CRUSH-205 class).
        pyo3::prepare_freethreaded_python();
        let json = r#"{"version":"0.1","functions":{"main":{"name":"main","params":[],"locals":[],"body":[{"op":"add"},{"op":"ret"}]}}}"#;
        let err = run_casm(json).expect_err("runtime fault must raise");
        let msg = err.to_string();
        assert!(
            msg.contains("VM Execution Error"),
            "wrong error path: {msg}"
        );
    }
}
