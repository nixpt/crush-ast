//! Built-in language executors for nanovm polyglot execution.
//!
//! Registers Lua (via mlua) and JavaScript (via quick-js) as default executors
//! so that `@lua { ... }` and `@js { ... }` blocks work out of the box without
//! requiring a platform runtime to register executors manually.

use crate::polyglot::exec::{json_to_runtime_value, runtime_value_to_json};
use crate::polyglot::executor_registry::{ExecutorResult, FunctionSignature, RuntimeExecutor};
use crate::RuntimeValue;
use std::collections::HashMap;
use std::sync::Mutex;

// ─── Lua executor (mlua) ──────────────────────────────────────────────────────

pub struct LuaExecutor {
    persistent: Mutex<HashMap<String, serde_json::Value>>,
}

impl LuaExecutor {
    pub fn new() -> Self {
        Self {
            persistent: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for LuaExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeExecutor for LuaExecutor {
    fn execute(
        &self,
        lang: &str,
        code: &str,
        variables: HashMap<String, RuntimeValue>,
    ) -> ExecutorResult {
        if !self.supports_language(lang) {
            return ExecutorResult::error(format!("LuaExecutor: unsupported language '{}'", lang));
        }

        let lua = match mlua::Lua::new_with(
            // EXO-77: restrict to safe subset — exclude io, os, package, debug, ffi.
            mlua::StdLib::STRING | mlua::StdLib::MATH | mlua::StdLib::TABLE | mlua::StdLib::COROUTINE,
            mlua::LuaOptions::default(),
        ) {
            Ok(l) => l,
            Err(e) => return ExecutorResult::error(format!("Lua init failed: {}", e)),
        };

        // Inject variables as Lua globals
        for (name, val) in &variables {
            let lua_val = runtime_value_to_lua(&lua, val);
            match lua_val {
                Ok(v) => {
                    if let Err(e) = lua.globals().set(name.as_str(), v) {
                        return ExecutorResult::error(format!(
                            "Lua: failed to set variable '{}': {}",
                            name, e
                        ));
                    }
                }
                Err(e) => {
                    return ExecutorResult::error(format!(
                        "Lua: failed to convert variable '{}': {}",
                        name, e
                    ))
                }
            }
        }

        // Inject persistent values
        {
            let persistent = self.persistent.lock().unwrap();
            for (name, val) in persistent.iter() {
                let rv = json_to_runtime_value(val);
                if let Ok(lv) = runtime_value_to_lua(&lua, &rv) {
                    let _ = lua.globals().set(name.as_str(), lv);
                }
            }
        }

        // Execute code
        match lua.load(code).eval::<mlua::Value>() {
            Ok(result) => {
                // Collect exports: any global that was set during execution
                let mut exports = HashMap::new();
                if let Ok(globals) = lua.globals().pairs::<String, mlua::Value>().collect::<Result<Vec<_>, _>>() {
                    for (k, v) in globals {
                        if let Ok(rv) = lua_value_to_runtime(&v) {
                            exports.insert(k, rv);
                        }
                    }
                }

                // Store exports as persistent
                {
                    let mut persistent = self.persistent.lock().unwrap();
                    for (k, v) in &exports {
                        persistent.insert(k.clone(), runtime_value_to_json(v));
                    }
                }

                let return_val = lua_value_to_runtime(&result).ok();
                ExecutorResult::success(return_val).with_exports(exports)
            }
            Err(e) => ExecutorResult::error(format!("Lua error: {}", e)),
        }
    }

    fn call_function(
        &self,
        lang: &str,
        function_name: &str,
        args: Vec<RuntimeValue>,
    ) -> ExecutorResult {
        if !self.supports_language(lang) {
            return ExecutorResult::error(format!("LuaExecutor: unsupported language '{}'", lang));
        }

        let lua = match mlua::Lua::new_with(
            // EXO-77: same restricted stdlib as execute()
            mlua::StdLib::STRING | mlua::StdLib::MATH | mlua::StdLib::TABLE | mlua::StdLib::COROUTINE,
            mlua::LuaOptions::default(),
        ) {
            Ok(l) => l,
            Err(e) => return ExecutorResult::error(format!("Lua init failed: {}", e)),
        };

        // Restore persistent state
        {
            let persistent = self.persistent.lock().unwrap();
            for (name, val) in persistent.iter() {
                let rv = json_to_runtime_value(val);
                if let Ok(lv) = runtime_value_to_lua(&lua, &rv) {
                    let _ = lua.globals().set(name.as_str(), lv);
                }
            }
        }

        let func: mlua::Function = match lua.globals().get(function_name) {
            Ok(f) => f,
            Err(_) => {
                return ExecutorResult::error(format!(
                    "Lua: function '{}' not found",
                    function_name
                ))
            }
        };

        let lua_args: Vec<mlua::Value> = args
            .iter()
            .filter_map(|a| runtime_value_to_lua(&lua, a).ok())
            .collect();

        match func.call::<_, mlua::MultiValue>(mlua::MultiValue::from_vec(lua_args)) {
            Ok(results) => {
                let val = results.into_iter().next()
                    .and_then(|v| lua_value_to_runtime(&v).ok());
                ExecutorResult::success(val)
            }
            Err(e) => ExecutorResult::error(format!("Lua call error: {}", e)),
        }
    }

    fn get_persistent(&self, name: &str) -> Option<RuntimeValue> {
        self.persistent
            .lock()
            .unwrap()
            .get(name)
            .map(json_to_runtime_value)
    }

    fn set_persistent(&self, name: &str, value: RuntimeValue) {
        self.persistent
            .lock()
            .unwrap()
            .insert(name.to_string(), runtime_value_to_json(&value));
    }

    fn list_persistent(&self) -> Vec<String> {
        self.persistent.lock().unwrap().keys().cloned().collect()
    }

    fn clear_persistent(&self) {
        self.persistent.lock().unwrap().clear();
    }

    fn get_exported_functions(&self, _lang: &str) -> Vec<FunctionSignature> {
        Vec::new()
    }

    fn supports_language(&self, lang: &str) -> bool {
        matches!(lang, "lua" | "lua5.4" | "lua54")
    }

    fn name(&self) -> &str {
        "lua-executor"
    }

    fn supported_languages(&self) -> Vec<&'static str> {
        vec!["lua", "lua5.4", "lua54"]
    }
}

fn runtime_value_to_lua<'lua>(
    lua: &'lua mlua::Lua,
    val: &RuntimeValue,
) -> Result<mlua::Value<'lua>, mlua::Error> {
    match val {
        RuntimeValue::Null => Ok(mlua::Value::Nil),
        RuntimeValue::Bool(b) => Ok(mlua::Value::Boolean(*b)),
        RuntimeValue::Int(i) => Ok(mlua::Value::Integer(*i)),
        RuntimeValue::Float(f) => Ok(mlua::Value::Number(*f)),
        RuntimeValue::String(s) => lua.create_string(s).map(mlua::Value::String),
        RuntimeValue::Ref(_) => Ok(mlua::Value::Nil), // Heap refs not supported cross-lang
    }
}

fn lua_value_to_runtime(val: &mlua::Value) -> Result<RuntimeValue, String> {
    match val {
        mlua::Value::Nil => Ok(RuntimeValue::Null),
        mlua::Value::Boolean(b) => Ok(RuntimeValue::Bool(*b)),
        mlua::Value::Integer(i) => Ok(RuntimeValue::Int(*i)),
        mlua::Value::Number(f) => Ok(RuntimeValue::Float(*f)),
        mlua::Value::String(s) => Ok(RuntimeValue::String(
            s.to_str().unwrap_or("").to_string(),
        )),
        _ => Err(format!("Cannot convert Lua value {:?} to RuntimeValue", val)),
    }
}

// ─── JavaScript executor (quick-js) ──────────────────────────────────────────

pub struct JsExecutor {
    persistent: Mutex<HashMap<String, serde_json::Value>>,
}

impl JsExecutor {
    pub fn new() -> Self {
        Self {
            persistent: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for JsExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeExecutor for JsExecutor {
    fn execute(
        &self,
        lang: &str,
        code: &str,
        variables: HashMap<String, RuntimeValue>,
    ) -> ExecutorResult {
        if !self.supports_language(lang) {
            return ExecutorResult::error(format!("JsExecutor: unsupported language '{}'", lang));
        }

        let context = quick_js::Context::new();
        let context = match context {
            Ok(c) => c,
            Err(e) => return ExecutorResult::error(format!("JS context init failed: {}", e)),
        };

        // Inject variables as JS globals via JSON
        for (name, val) in &variables {
            // EXO-78: validate variable name is a safe JS identifier before interpolation.
            if !is_valid_js_identifier(name) {
                return ExecutorResult::error(format!(
                    "JS: invalid variable name '{}' (must match [a-zA-Z_][a-zA-Z0-9_]*)",
                    name
                ));
            }
            let json_val = runtime_value_to_json(val);
            let inject = format!(
                "var {} = {};",
                name,
                serde_json::to_string(&json_val).unwrap_or_else(|_| "null".to_string())
            );
            if let Err(e) = context.eval(&inject) {
                return ExecutorResult::error(format!(
                    "JS: failed to inject variable '{}': {}",
                    name, e
                ));
            }
        }

        // Inject persistent values
        {
            let persistent = self.persistent.lock().unwrap();
            for (name, val) in persistent.iter() {
                let inject = format!(
                    "var {} = {};",
                    name,
                    serde_json::to_string(val).unwrap_or_else(|_| "null".to_string())
                );
                let _ = context.eval(&inject);
            }
        }

        // Execute code
        match context.eval(code) {
            Ok(result) => {
                let return_val = js_value_to_runtime(result);
                ExecutorResult::success(Some(return_val))
            }
            Err(e) => ExecutorResult::error(format!("JS error: {}", e)),
        }
    }

    fn call_function(
        &self,
        lang: &str,
        function_name: &str,
        args: Vec<RuntimeValue>,
    ) -> ExecutorResult {
        if !self.supports_language(lang) {
            return ExecutorResult::error(format!("JsExecutor: unsupported language '{}'", lang));
        }

        let context = match quick_js::Context::new() {
            Ok(c) => c,
            Err(e) => return ExecutorResult::error(format!("JS context init failed: {}", e)),
        };

        // Restore persistent state
        {
            let persistent = self.persistent.lock().unwrap();
            for (name, val) in persistent.iter() {
                let inject = format!(
                    "var {} = {};",
                    name,
                    serde_json::to_string(val).unwrap_or_else(|_| "null".to_string())
                );
                let _ = context.eval(&inject);
            }
        }

        // Build call expression
        let args_json: Vec<String> = args
            .iter()
            .map(|a| {
                serde_json::to_string(&runtime_value_to_json(a))
                    .unwrap_or_else(|_| "null".to_string())
            })
            .collect();
        let call_expr = format!("{}({})", function_name, args_json.join(", "));

        match context.eval(&call_expr) {
            Ok(result) => ExecutorResult::success(Some(js_value_to_runtime(result))),
            Err(e) => ExecutorResult::error(format!("JS call error: {}", e)),
        }
    }

    fn get_persistent(&self, name: &str) -> Option<RuntimeValue> {
        self.persistent
            .lock()
            .unwrap()
            .get(name)
            .map(json_to_runtime_value)
    }

    fn set_persistent(&self, name: &str, value: RuntimeValue) {
        self.persistent
            .lock()
            .unwrap()
            .insert(name.to_string(), runtime_value_to_json(&value));
    }

    fn list_persistent(&self) -> Vec<String> {
        self.persistent.lock().unwrap().keys().cloned().collect()
    }

    fn clear_persistent(&self) {
        self.persistent.lock().unwrap().clear();
    }

    fn get_exported_functions(&self, _lang: &str) -> Vec<FunctionSignature> {
        Vec::new()
    }

    fn supports_language(&self, lang: &str) -> bool {
        matches!(lang, "js" | "javascript" | "es6" | "ecmascript")
    }

    fn name(&self) -> &str {
        "js-executor"
    }

    fn supported_languages(&self) -> Vec<&'static str> {
        vec!["js", "javascript", "es6", "ecmascript"]
    }
}

fn js_value_to_runtime(val: quick_js::JsValue) -> RuntimeValue {
    match val {
        quick_js::JsValue::Null | quick_js::JsValue::Undefined => RuntimeValue::Null,
        quick_js::JsValue::Bool(b) => RuntimeValue::Bool(b),
        quick_js::JsValue::Int(i) => RuntimeValue::Int(i as i64),
        quick_js::JsValue::Float(f) => RuntimeValue::Float(f),
        quick_js::JsValue::String(s) => RuntimeValue::String(s),
        _ => RuntimeValue::Null,
    }
}

// ─── Registration helper ──────────────────────────────────────────────────────

/// Register all built-in executors (Lua + JS) with the global registry.
///
/// Called once during VM initialization. Safe to call multiple times —
/// the global registry is a OnceLock so subsequent calls are no-ops.
pub fn register_builtin_executors() {
    use crate::polyglot::executor_registry::register_executor;
    use std::sync::Arc;

    register_executor(Arc::new(LuaExecutor::new()));
    register_executor(Arc::new(JsExecutor::new()));
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// EXO-78: validate that a name is a safe JS identifier before string interpolation.
/// Accepts `[a-zA-Z_][a-zA-Z0-9_]*` only — no unicode, no reserved-word check needed
/// since the goal is preventing injection, not full JS spec compliance.
fn is_valid_js_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── EXO-78: JS identifier validation ─────────────────────────────────────

    #[test]
    fn valid_js_identifiers_accepted() {
        assert!(is_valid_js_identifier("foo"));
        assert!(is_valid_js_identifier("_bar"));
        assert!(is_valid_js_identifier("x1"));
        assert!(is_valid_js_identifier("camelCase"));
        assert!(is_valid_js_identifier("_"));
    }

    #[test]
    fn invalid_js_identifiers_rejected() {
        assert!(!is_valid_js_identifier(""));
        assert!(!is_valid_js_identifier("1foo"));          // starts with digit
        assert!(!is_valid_js_identifier("foo bar"));       // space
        assert!(!is_valid_js_identifier("foo;bar"));       // semicolon injection
        assert!(!is_valid_js_identifier("foo=bar"));       // assignment injection
        assert!(!is_valid_js_identifier("foo\nalert(1)")); // newline injection
    }

    // ── EXO-77: Lua sandbox ───────────────────────────────────────────────────

    #[test]
    fn lua_executor_rejects_io_access() {
        let exec = LuaExecutor::new();
        let result = exec.execute("lua", "io.open('/etc/passwd', 'r')", Default::default());
        // Should error because io stdlib is not loaded
        assert!(!result.success, "Lua io.open should be unavailable in sandbox");
    }

    #[test]
    fn lua_executor_rejects_os_execute() {
        let exec = LuaExecutor::new();
        let result = exec.execute("lua", "os.execute('id')", Default::default());
        assert!(!result.success, "Lua os.execute should be unavailable in sandbox");
    }

    #[test]
    fn lua_executor_allows_math_and_string() {
        let exec = LuaExecutor::new();
        let result = exec.execute("lua", "return math.floor(3.7)", Default::default());
        assert!(result.success, "Lua math should work in sandbox");
    }
}
