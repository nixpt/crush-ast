//! Polyglot execution support for nanovm
//!
//! This module provides the infrastructure for executing code in other languages
//! from within nanovm. It bridges the gap between the VM and platform runtimes.

use crate::RuntimeValue;
use std::collections::HashMap;

use super::executor_registry::{ExecutorResult, FunctionSignature};

/// Convert nanovm RuntimeValue to a JSON-compatible value for passing to external runtimes
pub fn runtime_value_to_json(value: &RuntimeValue) -> serde_json::Value {
    match value {
        RuntimeValue::Null => serde_json::Value::Null,
        RuntimeValue::Bool(b) => serde_json::json!(*b),
        RuntimeValue::Int(i) => serde_json::json!(*i),
        RuntimeValue::Float(f) => serde_json::json!(*f),
        RuntimeValue::String(s) => serde_json::json!(s),
        RuntimeValue::Ref(idx) => serde_json::json!({ "ref": idx }),
    }
}

/// Convert JSON value from external runtime back to nanovm RuntimeValue
pub fn json_to_runtime_value(json: &serde_json::Value) -> RuntimeValue {
    match json {
        serde_json::Value::Null => RuntimeValue::Null,
        serde_json::Value::Bool(b) => RuntimeValue::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                RuntimeValue::Int(i)
            } else if let Some(f) = n.as_f64() {
                RuntimeValue::Float(f)
            } else {
                RuntimeValue::Null
            }
        }
        serde_json::Value::String(s) => RuntimeValue::String(s.clone()),
        _ => RuntimeValue::Null,
    }
}

/// Convert HashMap of variables to JSON for external runtime
pub fn variables_to_json(
    vars: &HashMap<String, RuntimeValue>,
) -> serde_json::Map<String, serde_json::Value> {
    vars.iter()
        .map(|(k, v)| (k.clone(), runtime_value_to_json(v)))
        .collect()
}

/// Legacy type alias for backward compatibility
#[deprecated(
    since = "0.2.0",
    note = "Use ExecutorResult from executor_registry instead"
)]
pub type PolyglotExecutionResult = ExecutorResult;

/// Legacy success constructor for backward compatibility
#[deprecated(since = "0.2.0", note = "Use ExecutorResult::success instead")]
pub fn polyglot_success(value: Option<RuntimeValue>) -> ExecutorResult {
    ExecutorResult::success(value)
}

/// Legacy error constructor for backward compatibility  
#[deprecated(since = "0.2.0", note = "Use ExecutorResult::error instead")]
pub fn polyglot_error(msg: String) -> ExecutorResult {
    ExecutorResult::error(msg)
}

/// Trait for language runtime executors (legacy, use RuntimeExecutor from executor_registry)
pub trait LanguageExecutor: Send + Sync {
    /// Execute code in a specific language
    fn execute(
        &self,
        lang: &str,
        _code: &str,
        _variables: HashMap<String, RuntimeValue>,
    ) -> ExecutorResult {
        if lang != "crush" && lang != "cr" {
            return ExecutorResult::error(format!(
                "NativeExecutor only supports crush, got: {}",
                lang
            ));
        }

        // For crush, we just return success - actual execution happens in the VM itself
        // This is a placeholder - in real implementation, the VM would execute the code
        ExecutorResult::success(None)
    }

    fn call_function(
        &self,
        lang: &str,
        function_name: &str,
        args: Vec<RuntimeValue>,
    ) -> ExecutorResult;

    /// Get a persistent value
    fn get_persistent(&self, name: &str) -> Option<RuntimeValue>;

    /// Set a persistent value
    fn set_persistent(&self, name: &str, value: RuntimeValue);

    /// Get exported functions
    fn get_exported_functions(&self, lang: &str) -> Vec<FunctionSignature>;

    /// Check if a language is supported
    fn supports_language(&self, lang: &str) -> bool;
}

/// Native execution - executes crush code directly in the VM
pub struct NativeExecutor;

impl LanguageExecutor for NativeExecutor {
    fn execute(
        &self,
        lang: &str,
        _code: &str,
        _variables: HashMap<String, RuntimeValue>,
    ) -> ExecutorResult {
        if lang != "crush" && lang != "cr" {
            return ExecutorResult::error(format!(
                "NativeExecutor only supports crush, got: {}",
                lang
            ));
        }

        // For crush, we just return success - actual execution happens in the VM itself
        // This is a placeholder - in real implementation, the VM would execute the code
        ExecutorResult::success(None)
    }

    fn call_function(
        &self,
        lang: &str,
        _function_name: &str,
        _args: Vec<RuntimeValue>,
    ) -> ExecutorResult {
        if lang != "crush" && lang != "cr" {
            return ExecutorResult::error(format!(
                "NativeExecutor only supports crush, got: {}",
                lang
            ));
        }
        ExecutorResult::error("NativeExecutor does not support function calls yet".to_string())
    }

    fn get_persistent(&self, _name: &str) -> Option<RuntimeValue> {
        None
    }

    fn set_persistent(&self, _name: &str, _value: RuntimeValue) {
        // No-op for native executor
    }

    fn get_exported_functions(&self, _lang: &str) -> Vec<FunctionSignature> {
        Vec::new()
    }

    fn supports_language(&self, lang: &str) -> bool {
        lang == "crush" || lang == "cr"
    }
}
