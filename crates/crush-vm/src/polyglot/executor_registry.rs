//! Runtime executor abstraction for nanovm
//!
//! This module provides a runtime-pluggable executor for polyglot language execution.
//! It uses dependency inversion to avoid circular dependencies between nanovm and platform runtimes.
//!
//! Features:
//! - Cross-language function calls
//! - State persistence between executions
//! - Global registry pattern

use crate::RuntimeValue;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Function signature for cross-language calls
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub name: String,
    pub arg_names: Vec<String>,
    pub return_type: Option<String>,
}

/// Execution result from a language runtime
#[derive(Debug, Clone)]
pub struct ExecutorResult {
    pub value: Option<RuntimeValue>,
    pub stdout: String,
    pub stderr: String,
    pub exports: HashMap<String, RuntimeValue>,
    /// Exported functions for cross-language calls
    pub exported_functions: Vec<FunctionSignature>,
    pub success: bool,
    pub error: Option<String>,
}

impl ExecutorResult {
    pub fn success(value: Option<RuntimeValue>) -> Self {
        Self {
            value,
            stdout: String::new(),
            stderr: String::new(),
            exports: HashMap::new(),
            exported_functions: Vec::new(),
            success: true,
            error: None,
        }
    }

    pub fn error(msg: String) -> Self {
        Self {
            value: None,
            stdout: String::new(),
            stderr: msg.clone(),
            exports: HashMap::new(),
            exported_functions: Vec::new(),
            success: false,
            error: Some(msg),
        }
    }

    pub fn with_exports(mut self, exports: HashMap<String, RuntimeValue>) -> Self {
        self.exports = exports;
        self
    }

    pub fn with_functions(mut self, functions: Vec<FunctionSignature>) -> Self {
        self.exported_functions = functions;
        self
    }
}

/// Trait for language runtime executors
///
/// This trait is implemented by language runtimes (Python, JS, etc.)
/// and registered with the VM at runtime.
pub trait RuntimeExecutor: Send + Sync {
    /// Execute code in the specified language
    fn execute(
        &self,
        lang: &str,
        code: &str,
        variables: HashMap<String, RuntimeValue>,
    ) -> ExecutorResult;

    /// Call a function exported by a previous execution
    ///
    /// This enables cross-language function calls:
    /// @python { def add(a, b): return a + b }
    /// @js { result = call_lang("python", "add", [1, 2]) }
    fn call_function(
        &self,
        lang: &str,
        function_name: &str,
        args: Vec<RuntimeValue>,
    ) -> ExecutorResult;

    /// Get a persistent value by name (from previous executions)
    fn get_persistent(&self, name: &str) -> Option<RuntimeValue>;

    /// Set a persistent value (persists across executions within a session)
    fn set_persistent(&self, name: &str, value: RuntimeValue);

    /// List all persistent values
    fn list_persistent(&self) -> Vec<String>;

    /// Clear all persistent values
    fn clear_persistent(&self);

    /// Get exported functions from a previous execution
    fn get_exported_functions(&self, lang: &str) -> Vec<FunctionSignature>;

    /// Check if this executor supports a language
    fn supports_language(&self, lang: &str) -> bool;

    /// Get the name of this executor
    fn name(&self) -> &str;

    /// List supported languages
    fn supported_languages(&self) -> Vec<&'static str>;
}

/// Session state for persisting values across language blocks
#[derive(Debug, Default)]
pub struct SessionState {
    values: HashMap<String, RuntimeValue>,
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }

    pub fn get(&self, name: &str) -> Option<RuntimeValue> {
        self.values.get(name).cloned()
    }

    pub fn set(&mut self, name: String, value: RuntimeValue) {
        self.values.insert(name, value);
    }

    pub fn clear(&mut self) {
        self.values.clear();
    }

    pub fn keys(&self) -> Vec<String> {
        self.values.keys().cloned().collect()
    }
}

/// Global registry for runtime executors
///
/// This allows platform runtimes to register themselves with nanovm
/// without creating a circular dependency.
pub struct ExecutorRegistry {
    executors: RwLock<HashMap<String, Arc<dyn RuntimeExecutor>>>,
    /// Session state for persistence across language blocks
    session_state: RwLock<SessionState>,
    /// Cache of exported functions per language
    exported_functions: RwLock<HashMap<String, Vec<FunctionSignature>>>,
}

impl ExecutorRegistry {
    pub fn new() -> Self {
        Self {
            executors: RwLock::new(HashMap::new()),
            session_state: RwLock::new(SessionState::new()),
            exported_functions: RwLock::new(HashMap::new()),
        }
    }

    /// Register an executor for a language
    pub fn register(&self, executor: Arc<dyn RuntimeExecutor>) {
        for lang in executor.supported_languages() {
            self.executors
                .write()
                .insert(lang.to_string(), executor.clone());
        }
    }

    /// Get an executor for a language
    pub fn get(&self, lang: &str) -> Option<Arc<dyn RuntimeExecutor>> {
        self.executors.read().get(lang).cloned()
    }

    /// Check if a language is supported
    pub fn is_supported(&self, lang: &str) -> bool {
        self.executors.read().contains_key(lang)
    }

    /// List all supported languages
    pub fn supported_languages(&self) -> Vec<String> {
        self.executors.read().keys().cloned().collect()
    }

    /// Clear all registered executors (useful for testing)
    pub fn clear(&self) {
        self.executors.write().clear();
    }

    /// Execute code in a language
    pub fn execute(
        &self,
        lang: &str,
        code: &str,
        variables: HashMap<String, RuntimeValue>,
    ) -> ExecutorResult {
        if let Some(executor) = self.get(lang) {
            // Inject persistent variables into the execution
            let mut vars = variables;
            for key in self.session_state.read().keys() {
                if let Some(val) = self.session_state.read().get(&key) {
                    vars.insert(key, val.clone());
                }
            }

            let result = executor.execute(lang, code, vars);

            // Store exports as persistent values
            if result.success {
                for (key, value) in &result.exports {
                    self.session_state.write().set(key.clone(), value.clone());
                }
                // Store exported functions
                if !result.exported_functions.is_empty() {
                    self.exported_functions
                        .write()
                        .insert(lang.to_string(), result.exported_functions.clone());
                }
            }

            result
        } else {
            ExecutorResult::error(format!(
                "Language '{}' not supported. Available: {}",
                lang,
                self.supported_languages().join(", ")
            ))
        }
    }

    /// Call a function in a specific language
    ///
    /// This enables cross-language function calls:
    /// @python { def add(a, b): return a + b }
    /// @js { result = call_lang("python", "add", [1, 2]) }
    pub fn call_function(
        &self,
        lang: &str,
        function_name: &str,
        args: Vec<RuntimeValue>,
    ) -> ExecutorResult {
        if let Some(executor) = self.get(lang) {
            executor.call_function(lang, function_name, args)
        } else {
            ExecutorResult::error(format!(
                "Language '{}' not supported for function calls",
                lang
            ))
        }
    }

    /// Get a persistent value by name
    pub fn get_persistent(&self, name: &str) -> Option<RuntimeValue> {
        self.session_state.read().get(name)
    }

    /// Set a persistent value
    pub fn set_persistent(&self, name: &str, value: RuntimeValue) {
        self.session_state.write().set(name.to_string(), value);
    }

    /// List all persistent values
    pub fn list_persistent(&self) -> Vec<String> {
        self.session_state.read().keys()
    }

    /// Clear all persistent values
    pub fn clear_persistent(&self) {
        self.session_state.write().clear();
        self.exported_functions.write().clear();
    }

    /// Get exported functions for a language
    pub fn get_exported_functions(&self, lang: &str) -> Vec<FunctionSignature> {
        self.exported_functions
            .read()
            .get(lang)
            .cloned()
            .unwrap_or_default()
    }
}

impl Default for ExecutorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Global static executor registry
///
/// This is initialized once and can be accessed from anywhere.
/// Platform runtimes register themselves during initialization.
static EXECUTORS: std::sync::OnceLock<ExecutorRegistry> = std::sync::OnceLock::new();

/// Get the global executor registry
pub fn global_registry() -> &'static ExecutorRegistry {
    EXECUTORS.get_or_init(ExecutorRegistry::new)
}

/// Register an executor with the global registry
pub fn register_executor(executor: Arc<dyn RuntimeExecutor>) {
    global_registry().register(executor);
}

/// Execute code in a language using the global registry
pub fn execute_global(
    lang: &str,
    code: &str,
    variables: HashMap<String, RuntimeValue>,
) -> ExecutorResult {
    global_registry().execute(lang, code, variables)
}

/// Call a function in a language using the global registry
pub fn call_function_global(
    lang: &str,
    function_name: &str,
    args: Vec<RuntimeValue>,
) -> ExecutorResult {
    global_registry().call_function(lang, function_name, args)
}

/// Check if a language is supported globally
pub fn supports_language_global(lang: &str) -> bool {
    global_registry().is_supported(lang)
}

/// Get a persistent value globally
pub fn get_persistent_global(name: &str) -> Option<RuntimeValue> {
    global_registry().get_persistent(name)
}

/// Set a persistent value globally
pub fn set_persistent_global(name: &str, value: RuntimeValue) {
    global_registry().set_persistent(name, value);
}

/// Clear all persistent values globally
pub fn clear_persistent_global() {
    global_registry().clear_persistent();
}

/// Get exported functions for a language globally
pub fn get_exported_functions_global(lang: &str) -> Vec<FunctionSignature> {
    global_registry().get_exported_functions(lang)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockExecutor {
        lang: &'static str,
    }

    impl MockExecutor {
        fn new(lang: &'static str) -> Self {
            Self { lang }
        }
    }

    impl RuntimeExecutor for MockExecutor {
        fn execute(
            &self,
            lang: &str,
            code: &str,
            _variables: std::collections::HashMap<String, RuntimeValue>,
        ) -> ExecutorResult {
            if lang != self.lang {
                return ExecutorResult::error(format!("Expected lang {}, got {}", self.lang, lang));
            }
            ExecutorResult::success(Some(RuntimeValue::String(format!("executed: {}", code))))
        }

        fn call_function(
            &self,
            lang: &str,
            function_name: &str,
            _args: Vec<RuntimeValue>,
        ) -> ExecutorResult {
            if lang != self.lang {
                return ExecutorResult::error(format!("Expected lang {}", self.lang));
            }
            ExecutorResult::success(Some(RuntimeValue::String(format!(
                "called: {}",
                function_name
            ))))
        }

        fn get_persistent(&self, _name: &str) -> Option<RuntimeValue> {
            None
        }

        fn set_persistent(&self, _name: &str, _value: RuntimeValue) {}

        fn list_persistent(&self) -> Vec<String> {
            vec![]
        }

        fn clear_persistent(&self) {}

        fn get_exported_functions(&self, _lang: &str) -> Vec<FunctionSignature> {
            vec![]
        }

        fn supports_language(&self, lang: &str) -> bool {
            lang == self.lang
        }

        fn name(&self) -> &str {
            "mock-executor"
        }

        fn supported_languages(&self) -> Vec<&'static str> {
            vec![self.lang]
        }
    }

    #[test]
    fn test_executor_registry_register_and_get() {
        let registry = ExecutorRegistry::new();
        let executor: Arc<dyn RuntimeExecutor> = Arc::new(MockExecutor::new("testlang"));

        registry.register(executor);

        assert!(registry.is_supported("testlang"));
        assert!(registry.get("testlang").is_some());
    }

    #[test]
    fn test_executor_registry_unsupported_language() {
        let registry = ExecutorRegistry::new();

        assert!(!registry.is_supported("python"));
        assert!(registry.get("python").is_none());
    }

    #[test]
    fn test_execute_with_variables() {
        let registry = ExecutorRegistry::new();
        let executor: Arc<dyn RuntimeExecutor> = Arc::new(MockExecutor::new("testlang"));
        registry.register(executor);

        let mut vars = std::collections::HashMap::new();
        vars.insert("foo".to_string(), RuntimeValue::Int(42));

        let result = registry.execute("testlang", "print('hello')", vars);

        assert!(result.success);
    }

    #[test]
    fn test_persistent_values() {
        let registry = ExecutorRegistry::new();

        registry.set_persistent("key1", RuntimeValue::String("value1".to_string()));

        assert_eq!(
            registry.get_persistent("key1"),
            Some(RuntimeValue::String("value1".to_string()))
        );
        assert_eq!(registry.get_persistent("nonexistent"), None);

        let keys = registry.list_persistent();
        assert!(keys.contains(&"key1".to_string()));

        registry.clear_persistent();
        assert!(registry.list_persistent().is_empty());
    }

    #[test]
    fn test_execute_persists_exports() {
        let registry = ExecutorRegistry::new();
        let executor: Arc<dyn RuntimeExecutor> = Arc::new(MockExecutor::new("testlang"));
        registry.register(executor);

        let mut vars = std::collections::HashMap::new();
        let result = registry.execute("testlang", "code", vars);

        assert!(result.success);
    }

    #[test]
    fn test_call_function() {
        let registry = ExecutorRegistry::new();
        let executor: Arc<dyn RuntimeExecutor> = Arc::new(MockExecutor::new("testlang"));
        registry.register(executor);

        let result = registry.call_function(
            "testlang",
            "add",
            vec![RuntimeValue::Int(1), RuntimeValue::Int(2)],
        );

        assert!(result.success);
        assert!(result.value.is_some());
    }

    #[test]
    fn test_supported_languages() {
        let registry = ExecutorRegistry::new();
        let executor1: Arc<dyn RuntimeExecutor> = Arc::new(MockExecutor::new("python"));
        let executor2: Arc<dyn RuntimeExecutor> = Arc::new(MockExecutor::new("js"));

        registry.register(executor1);
        registry.register(executor2);

        let langs = registry.supported_languages();
        assert!(langs.contains(&"python".to_string()));
        assert!(langs.contains(&"js".to_string()));
    }

    #[test]
    fn test_global_registry_singleton() {
        clear_persistent_global();

        set_persistent_global("global_key", RuntimeValue::Float(3.14));
        let val = get_persistent_global("global_key");

        assert!(val.is_some());

        clear_persistent_global();
        assert!(get_persistent_global("global_key").is_none());
    }

    #[test]
    fn test_function_signature() {
        let sig = FunctionSignature {
            name: "add".to_string(),
            arg_names: vec!["a".to_string(), "b".to_string()],
            return_type: Some("int".to_string()),
        };

        assert_eq!(sig.name, "add");
        assert_eq!(sig.arg_names.len(), 2);
        assert_eq!(sig.return_type, Some("int".to_string()));
    }

    #[test]
    fn test_executor_result() {
        let success = ExecutorResult::success(Some(RuntimeValue::Int(42)));
        assert!(success.success);
        assert_eq!(success.value, Some(RuntimeValue::Int(42)));

        let err = ExecutorResult::error("something went wrong".to_string());
        assert!(!err.success);
        assert!(err.error.is_some());
    }
}
