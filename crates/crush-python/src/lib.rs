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

/// Run a CASM JSON string using the FastVM.
#[pyfunction]
fn run_casm(json: &str) -> PyResult<String> {
    let program: casm::Program = serde_json::from_str(json)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid CASM JSON: {}", e)))?;
        
    let yield_state = crush_vm::vm::run_fastvm(&program)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("VM Execution Error: {:?}", e)))?;
        
    Ok(format!("{:?}", yield_state))
}

/// Parse a CSON string.
#[pyfunction]
fn parse_cson(cson_str: &str) -> PyResult<String> {
    let mut parser = crush_cson::CsonParser::new(cson_str);
    let doc = parser.parse()
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;
    
    // For now, return a basic repr of the version and root to Python
    Ok(format!("CsonDocument(version={:?}, root={:?})", doc.version, doc.root))
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
    m.add_function(wrap_pyfunction!(run_casm, m)?)?;
    m.add_function(wrap_pyfunction!(parse_cson, m)?)?;
    m.add_function(wrap_pyfunction!(cast_version, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cast_version() {
        assert_eq!(cast_version(), "0.2");
    }

    #[test]
    fn test_parse_cast_valid() {
        let json = r#"{"cast_version":"0.2","entry":"main","functions":{}}"#;
        let result = parse_cast(json);
        assert!(result.is_ok());
        let val = result.unwrap();
        assert!(val.contains("0.2"));
    }

    #[test]
    fn test_parse_cast_invalid() {
        let json = r#"{"invalid_field":true}"#;
        let result = parse_cast(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_cast_valid() {
        let json = r#"{"cast_version":"0.2","entry":"main","functions":{}}"#;
        let result = validate_cast(json);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_validate_cast_invalid() {
        let json = r#"{"invalid_field":true}"#;
        let result = validate_cast(json);
        assert!(result.is_err());
    }
}

