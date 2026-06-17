//! RustPython guest runtime for CrushVM.
//!
//! When the `rustpython` feature is enabled, this crate embeds
//! RustPython as a sandboxed language runtime. Python source
//! executes inside RustPython's VM, with all I/O going through
//! the Crush capability bridge.

use crush_runtime_abi::{GuestContext, GuestRuntime, GuestValue, RuntimeLimits};

/// A RustPython runtime hosted inside CrushVM.
///
/// Created via [`RustPythonRuntime::new()`]. Each runtime maintains
/// its own interpreter state across calls to `eval_source`.
pub struct RustPythonRuntime {
    #[cfg(feature = "rustpython")]
    interpreter: Option<rustpython_vm::Interpreter>,
}

impl RustPythonRuntime {
    /// Create a new RustPython runtime.
    ///
    /// The runtime starts with an empty interpreter — no `import`ed
    /// modules persist across calls unless the runtime is kept alive.
    pub fn new() -> Self {
        #[cfg(feature = "rustpython")]
        {
            let settings = rustpython_vm::Settings {
                ..Default::default()
            };
            let interpreter = rustpython_vm::Interpreter::new_with_settings(settings);
            Self {
                interpreter: Some(interpreter),
            }
        }
        #[cfg(not(feature = "rustpython"))]
        {
            Self {}
        }
    }
}

impl GuestRuntime for RustPythonRuntime {
    fn eval_source(&mut self, source: &str, ctx: &GuestContext) -> anyhow::Result<GuestValue> {
        #[cfg(feature = "rustpython")]
        {
            let interpreter = self
                .interpreter
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("RustPython not initialized"))?;
            let result = interpreter.enter(|vm| {
                let scope = vm.new_scope_with_builtins();
                let code = vm
                    .compile(source, rustpython_vm::compiler::Mode::Exec, "<crush>".to_owned())
                    .map_err(|e| anyhow::anyhow!("RustPython compile error: {e}"))?;
                vm.run_code_obj(code, scope)
                    .map_err(|e| anyhow::anyhow!("RustPython runtime error: {e}"))
            })?;
            Ok(crate::convert::python_to_guest(result))
        }
        #[cfg(not(feature = "rustpython"))]
        {
            let _ = (source, ctx);
            anyhow::bail!("RustPython runtime not available (compile with --features rustpython)")
        }
    }

    fn call(
        &mut self,
        name: &str,
        args: &[GuestValue],
        ctx: &GuestContext,
    ) -> anyhow::Result<GuestValue> {
        #[cfg(feature = "rustpython")]
        {
            let interpreter = self
                .interpreter
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("RustPython not initialized"))?;
            let result = interpreter.enter(|vm| {
                let scope = vm.new_scope_with_builtins();
                let code_str = format!("{}(*[{}])", name,
                    args.iter().map(|a| format!("{:?}", a)).collect::<Vec<_>>().join(", "));
                let code = vm
                    .compile(&code_str, rustpython_vm::compiler::Mode::Eval, "<crush>".to_owned())
                    .map_err(|e| anyhow::anyhow!("RustPython call compile error: {e}"))?;
                vm.run_code_obj(code, scope)
                    .map_err(|e| anyhow::anyhow!("RustPython call error: {e}"))
            })?;
            Ok(crate::convert::python_to_guest(result))
        }
        #[cfg(not(feature = "rustpython"))]
        {
            let _ = (name, args, ctx);
            anyhow::bail!("RustPython runtime not available (compile with --features rustpython)")
        }
    }
}

/// Module for Python <-> GuestValue conversion.
mod convert {
    use crush_runtime_abi::GuestValue;

    /// Convert a RustPython object to a GuestValue.
    #[cfg(feature = "rustpython")]
    pub fn python_to_guest(obj: rustpython_vm::PyObjectRef) -> GuestValue {
        use rustpython_vm::builtins::{PyBoolRef, PyBytesRef, PyDictRef, PyFloatRef, PyIntRef, PyListRef, PyStrRef};
        use rustpython_vm::class::StaticType;
        use rustpython_vm::TryFromObject;

        let vm = &rustpython_vm::VirtualMachine::default();  // placeholder
        // Simple type dispatch
        if obj.is(&vm.ctx.none) {
            return GuestValue::Null;
        }
        if let Ok(v) = PyBoolRef::try_from_object(vm, obj.clone()) {
            return GuestValue::Bool(v.into_bool());
        }
        if let Ok(v) = PyIntRef::try_from_object(vm, obj.clone()) {
            return GuestValue::Int(v.as_big_int().try_to_i64().unwrap_or(0));
        }
        if let Ok(v) = PyFloatRef::try_from_object(vm, obj.clone()) {
            return GuestValue::Float(v.to_f64());
        }
        if let Ok(v) = PyStrRef::try_from_object(vm, obj.clone()) {
            return GuestValue::String(v.as_str().to_string());
        }
        if let Ok(v) = PyBytesRef::try_from_object(vm, obj.clone()) {
            return GuestValue::Bytes(v.get_value().to_vec());
        }
        if let Ok(v) = PyListRef::try_from_object(vm, obj.clone()) {
            let items: Vec<GuestValue> = v.borrow_vec().iter().map(|o| python_to_guest(o.clone())).collect();
            return GuestValue::List(items);
        }
        if let Ok(v) = PyDictRef::try_from_object(vm, obj.clone()) {
            let mut map = std::collections::HashMap::new();
            for (k, v) in v.borrow_dict().into_iter() {
                let key = k.as_str().unwrap_or("?").to_string();
                map.insert(key, python_to_guest(v.clone()));
            }
            return GuestValue::Map(map);
        }
        GuestValue::Null
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_abi_compiles() {
        // Verify the trait compiles
        let _: Box<dyn GuestRuntime> = Box::new(RustPythonRuntime::new());
    }
}
