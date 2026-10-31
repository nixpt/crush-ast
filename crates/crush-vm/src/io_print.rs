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

/// Python `print()`-shaped line: args joined with a single space, trailing newline.
pub fn format_py_print_line(parts: &[impl AsRef<str>]) -> String {
    let mut line = parts
        .iter()
        .map(|p| p.as_ref())
        .collect::<Vec<_>>()
        .join(" ");
    line.push('\n');
    line
}

/// `print(*values, sep=sep, end=end)` (CRUSH-zorro-38): `None` (crush null)
/// means CPython's defaults, `" "` and `"\n"`.
pub fn format_py_print_sep_end(args: &[crate::vm::Value]) -> String {
    let text = |v: Option<&crate::vm::Value>, default: &str| match v {
        None | Some(crate::vm::Value::Null) => default.to_string(),
        Some(v) => py_value_to_text(v),
    };
    let (sep, end) = (text(args.first(), " "), text(args.get(1), "\n"));
    let parts: Vec<String> = args.iter().skip(2).map(py_value_to_text).collect();
    let mut line = parts.join(&sep);
    line.push_str(&end);
    line
}

/// Render a [`crate::vm::Value`] the way CPython's `print` would for bool/None,
/// otherwise the canonical Crush `Display` / `as_text` form.
pub fn py_value_to_text(v: &crate::vm::Value) -> String {
    // `print(x)` is `str(x)`: a top-level string prints raw; everything else
    // (and every element inside a container) is `repr`.
    match v {
        crate::vm::Value::Str(s) => s.clone(),
        other => py_repr(other),
    }
}

/// CPython `repr()` for a crush value: `True`/`False`/`None`, quoted strings,
/// `[...]` lists, `(x,)` tuples, `{'k': v}` dicts (insertion order), and
/// floats with Python's switch to exponent notation.
pub fn py_repr(v: &crate::vm::Value) -> String {
    use crate::vm::Value;
    let seq = |items: &mut dyn Iterator<Item = String>| items.collect::<Vec<_>>().join(", ");
    match v {
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Null => "None".to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => py_float(*f),
        Value::Str(s) => py_str_repr(s),
        Value::Array(a) | Value::Vector(a) => format!("[{}]", seq(&mut a.borrow().iter().map(py_repr))),
        Value::List(l) => format!("[{}]", seq(&mut l.borrow().iter().map(py_repr))),
        Value::Tuple(t) if t.len() == 1 => format!("({},)", py_repr(&t[0])),
        Value::Tuple(t) => format!("({})", seq(&mut t.iter().map(py_repr))),
        Value::Set(s) if s.borrow().is_empty() => "set()".to_string(),
        Value::Set(s) => format!("{{{}}}", seq(&mut s.borrow().iter().map(py_repr))),
        Value::Map(m) => format!(
            "{{{}}}",
            seq(&mut m.borrow().iter().map(|(k, v)| format!("{}: {}", py_str_repr(k), py_repr(v))))
        ),
        other => other.as_text(),
    }
}

/// CPython float repr: shortest round-trip digits, `.0` on integral values,
/// exponent form (`1e+21`, `1.5e-07`) outside 1e-4 <= |f| < 1e16.
pub fn py_float(f: f64) -> String {
    if f.is_nan() {
        return "nan".to_string();
    }
    if f.is_infinite() {
        return if f > 0.0 { "inf" } else { "-inf" }.to_string();
    }
    let a = f.abs();
    if a == 0.0 || (1e-4..1e16).contains(&a) {
        return if f.fract() == 0.0 { format!("{f:.1}") } else { format!("{f}") };
    }
    // Rust `{:e}` gives the shortest digits as `1.5e-7`; CPython writes `1.5e-07`.
    let e = format!("{f:e}");
    let (mant, exp) = e.split_once('e').expect("exponent form");
    let exp: i32 = exp.parse().expect("exponent");
    format!("{mant}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs())
}

/// CPython `repr()` of a string: single quotes unless the text contains a
/// single quote and no double quote.
pub fn py_str_repr(s: &str) -> String {
    let q = if s.contains('\'') && !s.contains('"') { '"' } else { '\'' };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(q);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == q => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push(q);
    out
}

/// Render a [`RuntimeValue`] the way `io.print` expects for its argument.
///
/// This is the FastVM/JIT `io.print` formatter; a host that wires its own
/// `io.print` capability (e.g. the C API) should use it so output matches the
/// other engines. It mirrors the canonical `Value` Display used by the
/// scheduler/portable VM:
/// - `Null` → `"null"`
/// - `Bool` → `"true""/""false""`
/// - `Int` → bare digits
/// - `Float` → bare `f64` Display, with a forced `.0` suffix when the value
///   is finite and has no fractional part (so `3.0` prints as `3.0`, not `3`)
/// - `String` → the raw string content (no quotes)
/// - `Ref` → the arena object rendered the way the VM's `Display` renders the
///   equivalent `Value`: strings raw, arrays `[a, b]`, tuples `(a, b)`, maps and
///   structs `{k: v, …}`, `Vector[…]` / `Set{…}` / `List[…]`, bytes and handles
///   tagged. A dead slot still falls back to `"@<idx>"`.
pub fn runtime_value_to_text(v: &RuntimeValue, arena: &crate::memory::Arena) -> String {
    use crate::memory::Object;
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
        // Resolve the arena reference and render the heap value the way the
        // VM's `impl Display for Value` does (CRUSH-294 / GitHub #267). Before
        // this, every non-string ref printed as `@N` — silently wrong output
        // for arrays, maps and structs on FastVM and the JIT.
        RuntimeValue::Ref(idx) => match arena.get(*idx) {
            Some(Object::Str(s)) => s.clone(),
            Some(Object::Array(items)) => seq_text("[", "]", items, arena),
            Some(Object::Tuple(items)) => seq_text("(", ")", items, arena),
            Some(Object::Vector(items)) => seq_text("Vector[", "]", items, arena),
            Some(Object::Set(items)) => seq_text("Set{", "}", items, arena),
            Some(Object::List(items)) => {
                let inner: Vec<String> = items
                    .iter()
                    .map(|v| runtime_value_to_text(v, arena))
                    .collect();
                format!("List[{}]", inner.join(", "))
            }
            Some(Object::Map(map)) => {
                let inner: Vec<String> = map
                    .iter()
                    .map(|(k, v)| format!("{k}: {}", runtime_value_to_text(v, arena)))
                    .collect();
                format!("{{{}}}", inner.join(", "))
            }
            // A struct/object value prints as the map it is (the VM's
            // `Value::Record` / `Value::Map` render `{k: v, ...}`).
            Some(Object::Object { fields, .. }) => {
                let inner: Vec<String> = fields
                    .iter()
                    .map(|(k, v)| format!("{k}: {}", runtime_value_to_text(v, arena)))
                    .collect();
                format!("{{{}}}", inner.join(", "))
            }
            Some(Object::Bytes(b)) => format!("<{} bytes>", b.len()),
            Some(Object::Buffer(b)) => format!("<{} bytes>", b.len()),
            Some(Object::Handle(id)) => format!("<handle {id}>"),
            Some(Object::InterfaceHandle(id)) => format!("<handle {id}>"),
            Some(Object::Tagged { tag, value }) => {
                format!("{tag}({})", runtime_value_to_text(value, arena))
            }
            Some(Object::Result { ok, value }) => {
                let inner = runtime_value_to_text(value, arena);
                if *ok {
                    format!("ok({inner})")
                } else {
                    format!("err({inner})")
                }
            }
            // A dead slot is not reachable from a live program; keep the loud
            // marker rather than fabricating a value.
            None => format!("@{idx}"),
        },
    }
}

/// Render a sequence-like heap value (`[...]`, `(...)`, `Vector[...]`, …) with
/// the same element rendering as the VM's `Display`.
fn seq_text(
    open: &str,
    close: &str,
    items: &[RuntimeValue],
    arena: &crate::memory::Arena,
) -> String {
    let inner: Vec<String> = items
        .iter()
        .map(|v| runtime_value_to_text(v, arena))
        .collect();
    format!("{open}{}{close}", inner.join(", "))
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
    fn py_repr_matches_cpython() {
        use crate::vm::Value;
        let list = Value::new_array(vec![
            Value::Str("a".into()), Value::Int(1), Value::Float(2.5), Value::Bool(true), Value::Null,
            Value::new_array(vec![Value::Str("it's".into())]),
        ]);
        // python3 -c "print(['a', 1, 2.5, True, None, [\"it's\"]])"
        assert_eq!(py_value_to_text(&list), "['a', 1, 2.5, True, None, [\"it's\"]]");
        let d = Value::new_map(vec![("zeta".to_string(), Value::Int(1)), ("a b".to_string(), Value::Str("x\ny".into()))]);
        assert_eq!(py_value_to_text(&d), "{'zeta': 1, 'a b': 'x\\ny'}");
        assert_eq!(py_value_to_text(&Value::Tuple(vec![Value::Int(1)])), "(1,)");
        assert_eq!(py_value_to_text(&Value::Str("raw".into())), "raw");
        for (f, want) in [(6.0, "6.0"), (0.1 + 0.2, "0.30000000000000004"), (1e21, "1e+21"), (1.5e-7, "1.5e-07"),
                          (1e16, "1e+16"), (123456789012345.0, "123456789012345.0"), (0.0001, "0.0001"), (-0.0, "-0.0")] {
            assert_eq!(py_float(f), want, "{f}");
        }
    }

    #[test]
    fn format_py_print_line_joins_with_space() {
        assert_eq!(format_py_print_line(&["1".to_string(), "2".to_string(), "a".to_string()]), "1 2 a\n");
        assert_eq!(format_py_print_line(&[] as &[String]), "\n");
    }

    #[test]
    fn runtime_value_to_text_matches_canonical_rendering() {
        let arena = crate::memory::Arena::new();
        assert_eq!(runtime_value_to_text(&RuntimeValue::Null, &arena), "null");
        assert_eq!(runtime_value_to_text(&RuntimeValue::Bool(true), &arena), "true");
        assert_eq!(runtime_value_to_text(&RuntimeValue::Int(42), &arena), "42");
        assert_eq!(runtime_value_to_text(&RuntimeValue::String("hi".to_string()), &arena), "hi");
    }

    // CRUSH-294 / GitHub #267: heap values render their value, not `@N`.
    #[test]
    fn runtime_value_to_text_renders_heap_values_like_the_vm() {
        use crate::memory::{Arena, Object};
        let mut arena = Arena::new();
        let ints = |a: &mut Arena, xs: &[i64]| {
            RuntimeValue::Ref(a.alloc(Object::Array(
                xs.iter().map(|i| RuntimeValue::Int(*i)).collect(),
            )))
        };
        let one = ints(&mut arena, &[1]);
        let two_three = ints(&mut arena, &[2, 3]);
        let nested = RuntimeValue::Ref(arena.alloc(Object::Array(vec![one, two_three])));
        assert_eq!(runtime_value_to_text(&nested, &arena), "[[1], [2, 3]]");

        let map = RuntimeValue::Ref(
            arena.alloc(Object::Map(
                [("a".to_string(), RuntimeValue::Int(1))]
                    .into_iter()
                    .collect(),
            )),
        );
        assert_eq!(runtime_value_to_text(&map, &arena), "{a: 1}");

        let obj = RuntimeValue::Ref(
            arena.alloc(Object::Object {
                lang: "crush".into(),
                class_name: "P".into(),
                fields: [("x".to_string(), RuntimeValue::Int(5))]
                    .into_iter()
                    .collect(),
            }),
        );
        assert_eq!(runtime_value_to_text(&obj, &arena), "{x: 5}");

        // A string renders raw (no quotes), including inside a container.
        let s = RuntimeValue::Ref(arena.alloc(Object::Str("hi".to_string())));
        let arr_s = RuntimeValue::Ref(arena.alloc(Object::Array(vec![s])));
        assert_eq!(runtime_value_to_text(&arr_s, &arena), "[hi]");
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
            cosm: None,
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
