//! Shared `io.print` formatting logic used by every Crush backend.
//!
//! The trailing newline is defined in exactly one place: [`format_io_print_line`].
//! All runtimes (scheduler, portable VM, FastVM) and the AOT code generators
//! should reproduce this behavior so a program's stdout is identical across
//! backends.

use crate::RuntimeValue;

/// Concatenate already-rendered `io.print` arguments and append the single
/// trailing newline that every backend must emit.
///
/// This is the single source of truth for the newline; value rendering is
/// left to each backend so that `Value`, `RuntimeValue`, and generated
/// backends can reuse their own text formatters.
pub fn format_io_print_line(parts: &[impl AsRef<str>]) -> String {
    let mut line = String::new();
    for part in parts {
        line.push_str(part.as_ref());
    }
    line.push('\n');
    line
}

/// Render a [`RuntimeValue`] the same way `io.print` expects for its argument.
///
/// Mirrors the canonical `Value` Display used by the scheduler/portable VM:
/// - `Null` → `"null"`
/// - `Bool` → `"true""/""false""`
/// - `Int` → bare digits
/// - `Float` → bare `f64` Display, with a forced `.0` suffix when the value
///   is finite and has no fractional part (so `3.0` prints as `3.0`, not `3`)
/// - `String` → the raw string content (no quotes)
/// - `Ref` → the arena reference index as `"@<idx>"`, or the referenced
///   string if the arena contains `Object::Str`.
pub(crate) fn runtime_value_to_text(v: &RuntimeValue, arena: &crate::memory::Arena) -> String {
    match v {
        RuntimeValue::Null => "null".to_string(),
        RuntimeValue::Bool(b) => b.to_string(),
        RuntimeValue::Int(i) => i.to_string(),
        RuntimeValue::Float(f) => {
            if f.is_finite() && f.fract() == 0.0 {
                format!("{f:.1}")
            } else {
                f.to_string()
            }
        }
        RuntimeValue::String(s) => s.clone(),
        RuntimeValue::Ref(idx) => {
            if let Some(crate::memory::Object::Str(s)) = arena.get(*idx) {
                s.clone()
            } else {
                format!("@{idx}")
            }
        }
    }
}

#[cfg(feature = "native-plugins")]
pub use fastvm_print::PrintCap;

#[cfg(feature = "native-plugins")]
mod fastvm_print {
    use super::{format_io_print_line, runtime_value_to_text};
    use crate::RuntimeValue;
    use crate::fastvm::{Capability, Hal};
    use crate::memory::Arena;
    use std::sync::{Arc, Mutex};

    /// FastVM capability that implements `io.print` for test harnesses.
    ///
    /// The captured output can be inspected via [`PrintCap::output`]. This is
    /// intended for differential testing and harnesses; real stdout emission
    /// would require a `Capability` trait extension or host-provided output
    /// channel.
    #[derive(Debug)]
    pub struct PrintCap {
        output: Arc<Mutex<String>>,
    }

    impl PrintCap {
        pub fn new() -> Self {
            Self {
                output: Arc::new(Mutex::new(String::new())),
            }
        }

        pub fn with_output(output: Arc<Mutex<String>>) -> Self {
            Self { output }
        }

        pub fn output(&self) -> String {
            self.output.lock().unwrap().clone()
        }
    }

    impl Default for PrintCap {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Capability for PrintCap {
        fn name(&self) -> &str {
            "io.print"
        }

        fn call(
            &self,
            arena: &mut Arena,
            args: Vec<RuntimeValue>,
            _hal: Arc<dyn Hal>,
        ) -> anyhow::Result<RuntimeValue> {
            let parts: Vec<String> = args.iter().map(|v| runtime_value_to_text(v, arena)).collect();
            let line = format_io_print_line(&parts);
            self.output.lock().unwrap().push_str(&line);
            Ok(RuntimeValue::Null)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_io_print_line_appends_single_newline() {
        assert_eq!(format_io_print_line(&["hello".to_string()]), "hello\n");
        assert_eq!(format_io_print_line(&["a".to_string(), "b".to_string()]), "ab\n");
        assert_eq!(format_io_print_line(&[] as &[String]), "\n");
    }

    #[test]
    fn runtime_value_to_text_matches_canonical_rendering() {
        let arena = crate::memory::Arena::new();
        assert_eq!(runtime_value_to_text(&RuntimeValue::Null, &arena), "null");
        assert_eq!(runtime_value_to_text(&RuntimeValue::Bool(true), &arena), "true");
        assert_eq!(runtime_value_to_text(&RuntimeValue::Int(42), &arena), "42");
        assert_eq!(runtime_value_to_text(&RuntimeValue::String("hi".to_string()), &arena), "hi");
    }

    #[cfg(feature = "native-plugins")]
    #[test]
    fn fastvm_printcap_emits_trailing_newline() {
        use crate::fastvm::{FastVM, LoweredProgram};
        use std::sync::Arc;

        let program = casm::Program {
            version: "1.0".to_string(),
            functions: {
                let mut functions = std::collections::HashMap::new();
                functions.insert(
                    "main".to_string(),
                    casm::Function {
                        params: vec![],
                        locals: vec![],
                        type_hints: None,
                        body: vec![
                            casm::Instruction {
                                op: "push_str".to_string(),
                                lang: None,
                                meta: None,
                                args: serde_json::json!({"value": "hello"}),
                            },
                            casm::Instruction {
                                op: "cap_call".to_string(),
                                lang: None,
                                meta: None,
                                args: serde_json::json!({"name": "io.print", "argc": 1}),
                            },
                            casm::Instruction {
                                op: "halt".to_string(),
                                lang: None,
                                meta: None,
                                args: serde_json::json!({}),
                            },
                        ],
                    },
                );
                functions
            },
            manifest: casm::Manifest {
                permissions: vec!["io.print".to_string()],
            },
            lang: None,
        };

        #[derive(Debug)]
        struct DummyHal;
        impl crate::fastvm::Hal for DummyHal {}

        let lowered = crate::fastvm::lower_program(&program).expect("lower program");
        let print_cap = Arc::new(PrintCap::new());
        let caps: Vec<Arc<dyn crate::fastvm::Capability>> = vec![print_cap.clone()];
        let mut vm = FastVM::new(lowered, caps, Arc::new(DummyHal));
        let _result = vm.run(1_000_000);
        assert_eq!(print_cap.output(), "hello\n");
    }
}
