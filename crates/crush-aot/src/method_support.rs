//! Shared method kernel embedding and the private scalar/string C ABI.
pub(crate) const NAMES: &[&str] = &[
    "str.to_lower",
    "str.to_upper",
    "str.trim",
    "str.replace",
    "str.contains",
    "str.starts_with",
    "str.ends_with",
    "rust.str.starts_with",
    "rust.str.ends_with",
    "rust.num",
    "math.abs",
    "math.sqrt",
    "math.floor",
    "math.ceil",
    "math.round",
    "math.pow",
];
pub(crate) fn id(name: &str) -> Option<usize> {
    NAMES.iter().position(|n| *n == name)
}
pub(crate) fn used(program: &casm::Program) -> bool {
    program.functions.values().flat_map(|f| &f.body).any(|i| {
        i.op == "cap_call"
            && i.args
                .get("name")
                .and_then(|v| v.as_str())
                .is_some_and(|n| id(n).is_some())
    })
}
fn kernel_source() -> String {
    // Inner module documentation is emitted as comments because this source is
    // embedded after the module's opening brace in generated compilation units.
    crush_vm::rust_methods::CORE_SOURCE.replace("//!", "//")
}
pub(crate) fn emit_rust(out: &mut String) {
    out.push_str("#[allow(dead_code)]\nmod rust_methods_core {\n");
    out.push_str(&kernel_source());
    out.push_str("\n}\n");
    out.push_str(r#"
fn rust_method_call(name: &str, args: Vec<RuntimeValue>) -> RuntimeValue {
    use rust_methods_core::{Input, Output};
    let strings: Vec<_> = args.iter().map(|v| if name.starts_with("str.") { Some(v.to_string()) } else { None }).collect();
    let input: Vec<_> = args.iter().zip(&strings).map(|(v,s)| {
        if let Some(s) = s { return Input::Text(s); }
        match v { RuntimeValue::Null => Input::Null, RuntimeValue::Int(i) => Input::Int(*i), RuntimeValue::Float(f) => Input::Float(*f), RuntimeValue::Bool(b) => Input::Bool(*b), RuntimeValue::String(s) => Input::Text(s), _ => Input::Invalid }
    }).collect();
    match rust_methods_core::call(name, &input) {
        Ok(Output::Int(i)) => RuntimeValue::Int(i), Ok(Output::Float(f)) => RuntimeValue::Float(f),
        Ok(Output::Bool(b)) => RuntimeValue::Bool(b), Ok(Output::Text(s)) => RuntimeValue::String(s),
        Err(e) => { eprintln!("method capability {name}: {e}"); std::process::exit(1); }
    }
}
"#);
}
pub(crate) fn bridge_source() -> String {
    let mut out = format!("mod core {{\n{}\n}}\n", kernel_source());
    out.push_str("const NAMES: &[&str] = &[\n");
    for name in NAMES {
        out.push_str(&format!("{name:?},\n"));
    }
    out.push_str("];\n");
    out.push_str(include_str!("method_bridge.rs.in"));
    out
}

fn has_nul(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::String(s) => s.contains('\0'),
        serde_json::Value::Array(a) => a.iter().any(has_nul),
        serde_json::Value::Object(o) => o.values().any(has_nul),
        _ => false,
    }
}

/// The exported entrypoint uses C strings. Keep NUL-capable root results
/// outside that transport; unit-returning Rust programs may still print NULs.
pub(crate) fn check_rust_result(program: &casm::Program) -> Result<(), crate::UnsupportedOps> {
    if !used(program) {
        return Ok(());
    }
    fn scalar_root(name: &str, program: &casm::Program, visiting: &mut Vec<String>) -> bool {
        if visiting.iter().any(|n| n == name) {
            return false;
        }
        let Some(f) = program.functions.get(name) else {
            return false;
        };
        visiting.push(name.into());
        let has_return = f
            .body
            .iter()
            .any(|i| matches!(i.op.as_str(), "ret" | "return"));
        let result = has_return
            && f.body
                .iter()
                .enumerate()
                .filter(|(_, i)| matches!(i.op.as_str(), "ret" | "return"))
                .all(|(n, _)| {
                    let Some(p) = n.checked_sub(1).and_then(|n| f.body.get(n)) else {
                        return false;
                    };
                    match p.op.as_str() {
                        "push_null" | "push_int" | "push_float" | "push_bool" => true,
                        "call" => p
                            .args
                            .get("function")
                            .and_then(|v| v.as_str())
                            .is_some_and(|n| scalar_root(n, program, visiting)),
                        "cap_call" => {
                            p.args
                                .get("name")
                                .and_then(|v| v.as_str())
                                .is_some_and(|n| {
                                    n.starts_with("math.")
                                        || matches!(
                                            n,
                                            "rust.num"
                                                | "rust.str.starts_with"
                                                | "rust.str.ends_with"
                                                | "str.contains"
                                                | "str.starts_with"
                                                | "str.ends_with"
                                        )
                                        || crush_vm::caps::capabilities()
                                            .get(n)
                                            .is_some_and(|s| !s.returns)
                                })
                        }
                        _ => false,
                    }
                });
        visiting.pop();
        result
    }
    let nul_source = program.functions.values().flat_map(|f| &f.body).any(|i| {
        has_nul(&i.args)
            || i.op == "cap_call"
                && matches!(
                    i.args.get("name").and_then(|v| v.as_str()),
                    Some("io.read" | "conv.chr")
                )
    });
    if nul_source && !scalar_root("main", program, &mut Vec::new()) {
        return Err(crate::UnsupportedOps {
            backend: "Rust",
            ops: vec![
                "NUL-capable root result needs a length-aware host result ABI (CRUSH-xps-13)"
                    .into(),
            ],
        });
    }
    Ok(())
}

/// Refuse known loss of information before a C module can load. With the
/// closed entrypoint (no arguments), all text originates in constants or these
/// producer operations; shared transformations cannot manufacture a NUL from
/// non-NUL inputs. Unknown caps/opcodes are independently refused by codegen.
pub(crate) fn check_c(program: &casm::Program) -> Result<(), crate::UnsupportedOps> {
    if !used(program) {
        return Ok(());
    }
    let mut ops = Vec::new();
    if !cfg!(target_os = "linux") {
        ops.push(
            "private Rust method bridge currently requires native Linux (CRUSH-xps-13)".into(),
        );
    }
    for f in program.functions.values() {
        for (index, i) in f.body.iter().enumerate() {
            if has_nul(&i.args) {
                ops.push("C method input contains NUL; length-aware C Value storage is required (CRUSH-xps-13)".into());
            }
            let name = i.args.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if i.op == "cap_call" && name.starts_with("str.") && id(name).is_some() {
                let count = crush_vm::rust_methods_core::arity(name).unwrap();
                let proven = i
                    .meta
                    .as_ref()
                    .and_then(|m| m.get("rust_string_inputs_proven"))
                    .and_then(|v| v.as_bool())
                    == Some(true);
                let literals = index >= count
                    && f.body[index - count..index]
                        .iter()
                        .all(|p| p.op == "push_str");
                if !proven && !literals {
                    ops.push(
                        "C string capability coercion needs proven string inputs (CRUSH-xps-13)"
                            .into(),
                    );
                }
            }

            if (i.op == "cap_call" && matches!(name, "io.read" | "conv.chr"))
                || matches!(i.op.as_str(), "io_read" | "chr" | "conv_chr")
            {
                ops.push("C method input has unproven external/character text representability (CRUSH-xps-13)".into());
            }
        }
    }
    ops.sort();
    ops.dedup();
    if ops.is_empty() {
        Ok(())
    } else {
        Err(crate::UnsupportedOps { backend: "C", ops })
    }
}

pub(crate) fn emit_c(out: &mut String) {
    out.push_str(r#"
/* Private xps-13 ABI. Buffers remain live during the call; no callbacks/GC. */
typedef struct { uint32_t tag, reserved; int64_t i; double f; const uint8_t* p; size_t n; } CrushSupportInput;
typedef struct { uint32_t status, tag; int64_t i; double f; uint8_t* p; size_t n; } CrushSupportOutput;
extern CrushSupportOutput crush_support_call(uint32_t op, const CrushSupportInput* args, size_t argc);
extern void crush_support_free(uint8_t* p, size_t n);
static Value _shared_method(uint32_t op, Value* args, size_t argc) {
    CrushSupportInput in[4] = {0};
    for (size_t j=0; j<argc; j++) {
        Value v=args[j];
        switch(v.tag) {
            case TAG_NULL: in[j].tag=0; break;
            case TAG_INT: in[j].tag=1; in[j].i=v.i; break;
            case TAG_FLOAT: in[j].tag=2; in[j].f=v.f; break;
            case TAG_BOOL: in[j].tag=3; in[j].i=v.b; break;
            case TAG_STRING: in[j].tag=4; in[j].p=(const uint8_t*)v.s; in[j].n=strlen(v.s); break;
            default: in[j].tag=5; break;
        }
    }
    CrushSupportOutput r=crush_support_call(op,in,argc);
    if (r.status) { fputs("method capability: ",stderr); if(r.p) fwrite(r.p,1,r.n,stderr); fputc('\n',stderr); crush_support_free(r.p,r.n); exit(1); }
    Value result=mk_null();
    switch(r.tag) {
        case 1: result=mk_int(r.i); break;
        case 2: result=mk_float(r.f); break;
        case 3: result=mk_bool(r.i!=0); break;
        case 4: {
            if (r.n && memchr(r.p,0,r.n)) { crush_support_free(r.p,r.n); fputs("C method output contains NUL (CRUSH-xps-13)\n",stderr); exit(1); }
            char* text=_str_new(r.n); if(r.n) memcpy(text,r.p,r.n); text[r.n]=0;
            result=mk_string(text); break;
        }
        default: crush_support_free(r.p,r.n); fputs("invalid method bridge result\n",stderr); exit(1);
    }
    crush_support_free(r.p,r.n);
    return result;
}
"#);
}

#[cfg(test)]
mod tests {
    // Compile and exercise the actual generated ABI implementation in-process.
    mod core {
        pub use crush_vm::rust_methods_core::*;
    }
    const NAMES: &[&str] = super::NAMES;
    include!("method_bridge.rs.in");
    fn arg(tag: u32, i: i64, bytes: &[u8]) -> AbiInput {
        AbiInput {
            tag,
            reserved: 0,
            i,
            f: 0.0,
            p: bytes.as_ptr(),
            n: bytes.len(),
        }
    }
    #[test]
    fn bridge_validates_arity_tags_utf8_lengths_and_reserved_fields() {
        let mut a = arg(4, 0, b"ABC");
        for kind in 0..6 {
            a = arg(4, 0, b"ABC");
            match kind {
                0 => a.tag = 255,
                1 => a.reserved = 1,
                2 => {
                    a.p = std::ptr::null();
                    a.n = 1;
                }
                3 => a.n = usize::MAX,
                4 => {
                    a.p = b"\xff".as_ptr();
                    a.n = 1;
                }
                _ => {
                    a.tag = 3;
                    a.i = 2;
                }
            }
            let r = unsafe { crush_support_call(0, &a, 1) };
            assert_eq!(r.status, 1);
            assert!(!r.p.is_null());
            unsafe {
                crush_support_free(r.p, r.n);
            }
        }
        for (op, ptr, argc) in [
            (999, &a as *const _, 1),
            (0, std::ptr::null(), 1),
            (0, &a as *const _, 5),
        ] {
            let r = unsafe { crush_support_call(op, ptr, argc) };
            assert_eq!(r.status, 1);
            unsafe {
                crush_support_free(r.p, r.n);
            }
        }
    }
    #[test]
    fn bridge_preserves_nul_and_owns_repeated_text_results() {
        let a = arg(4, 0, b"A\0B");
        for _ in 0..1000 {
            let r = unsafe { crush_support_call(0, &a, 1) };
            assert_eq!((r.status, r.tag, r.n), (0, 4, 3));
            assert_eq!(unsafe { std::slice::from_raw_parts(r.p, r.n) }, b"a\0b");
            unsafe {
                crush_support_free(r.p, r.n);
            }
        }
        unsafe {
            crush_support_free(std::ptr::null_mut(), 0);
        }
    }
    #[test]
    fn unsupported_value_types_reach_canonical_kernel_validation() {
        let args = [
            arg(4, 0, b"pow"),
            arg(4, 0, b"i8"),
            arg(5, 0, b""),
            arg(1, 3, b""),
        ];
        let r = unsafe { crush_support_call(9, args.as_ptr(), 4) };
        assert_eq!(r.status, 1);
        assert_eq!(
            unsafe { std::slice::from_raw_parts(r.p, r.n) },
            b"expected integer at index 2"
        );
        unsafe {
            crush_support_free(r.p, r.n);
        }
    }
    #[test]
    fn c_representation_gate_precedes_codegen() {
        for source in [
            "fn main() { return str.to_lower(\"A\0B\"); }",
            "fn main() { return str.to_lower(io.read()); }",
            "fn main() { return str.to_lower(conv.chr(0)); }",
        ] {
            let program = crush_frontend::compile_crush_source(source).unwrap();
            let error = crate::codegen_c::gen_c_source(&program)
                .unwrap_err()
                .to_string();
            assert!(error.contains("CRUSH-xps-13"), "{error}");
        }
    }
}

#[cfg(test)]
mod unwind_test {
    mod core {
        pub use crush_vm::rust_methods_core::*;
    }
    // A corrupted internal registry forces an unexpected implementation panic.
    const NAMES: &[&str] = &["deliberately_missing_kernel"];
    include!("method_bridge.rs.in");
    #[test]
    fn unexpected_panics_return_owned_errors_without_crossing_c() {
        let arg = AbiInput {
            tag: 0,
            reserved: 0,
            i: 0,
            f: 0.0,
            p: std::ptr::null(),
            n: 0,
        };
        let r = unsafe { crush_support_call(0, &arg, 1) };
        assert_eq!(r.status, 2);
        assert_eq!(
            unsafe { std::slice::from_raw_parts(r.p, r.n) },
            b"Rust support panic"
        );
        unsafe {
            crush_support_free(r.p, r.n);
        }
    }
}
