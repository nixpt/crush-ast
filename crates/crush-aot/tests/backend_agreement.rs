//! Edge cases where the Rust and C AOT backends must give the same answer (PR #126 review).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crush_aot::{AotCompiler, Module};
use crush_vm::RuntimeValue;

fn cc_available() -> bool {
    Command::new("gcc").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

fn run(so: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_crush-aot-runner")).arg(so).output().expect("spawn crush-aot-runner")
}

/// `(backend, built .so)` for each backend available here.
fn build_both(program: &casm::Program, name: &str) -> Vec<(&'static str, PathBuf)> {
    let compiler = AotCompiler::new();
    let mut out = vec![("rust", compiler.compile_casm(program, &format!("{name}_rust")).expect("rust backend"))];
    if cc_available() {
        out.push(("c", compiler.compile_c(program, &format!("{name}_c"), "gcc").expect("c backend")));
    }
    out
}

fn compile(source: &str) -> casm::Program {
    crush_frontend::compile_crush_source(source).expect("frontend")
}

// `crush_run` copied a string result into a fixed 512-byte buffer, so a longer string
// returned from `main` came back truncated on the C backend.
#[test]
fn long_string_result_is_returned_whole() {
    let program = compile(
        r#"fn main() { let s = ""; let i = 0; while i < 100 { s = s + "0123456789"; i = i + 1; } return s; }"#,
    );
    for (backend, so) in build_both(&program, "long_result") {
        let result = Module::load(&so).expect("load").call_main().expect("call_main");
        assert_eq!(result, RuntimeValue::String("0123456789".repeat(100)), "{backend}");
    }
}

// `[] + []` passed a NULL data pointer to memcpy on the C backend.
#[test]
fn concatenating_empty_arrays() {
    let program = compile("fn main() { let e = [] + []; let f = e + [7]; return len(e) * 10 + f[0]; }");
    for (backend, so) in build_both(&program, "empty_concat") {
        let result = Module::load(&so).expect("load").call_main().expect("call_main");
        assert_eq!(result, RuntimeValue::Int(7), "{backend}");
    }
}

// INT64_MIN / -1 and % -1 overflow. The Rust backend reports it; the C backend used to
// die with SIGFPE.
#[test]
fn int_min_divided_by_minus_one_is_an_overflow_error() {
    for op in ["/", "%"] {
        let program = compile(&format!(
            "fn n(x) {{ return x; }} fn main() {{ return n(-9223372036854775807 - 1) {op} n(-1); }}"
        ));
        for (backend, so) in build_both(&program, if op == "/" { "min_div" } else { "min_mod" }) {
            let out = run(&so);
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert_eq!(out.status.code(), Some(1), "{backend} {op}: {stderr}");
            assert!(stderr.contains("arithmetic overflow"), "{backend} {op}: {stderr}");
        }
    }
}

/// Rename functions in a compiled program, call sites included. The Crush frontend only
/// accepts ASCII names, but polyglot walkers hand the backends CASM with others.
fn rename_functions(program: &mut casm::Program, renames: &[(&str, &str)]) {
    for (from, to) in renames {
        let f = program.functions.remove(*from).expect("function to rename");
        program.functions.insert(to.to_string(), f);
    }
    for f in program.functions.values_mut() {
        for instr in &mut f.body {
            if instr.op == "call" {
                if let Some(target) = instr.args.get("function").and_then(|t| t.as_str()) {
                    if let Some((_, to)) = renames.iter().find(|(from, _)| *from == target) {
                        instr.args["function"] = serde_json::Value::from(*to);
                    }
                }
            }
        }
    }
}

// Every non-ASCII character used to become `_`, so `café` and `cafè` (or `π` and `_`)
// turned into one identifier and the Rust build failed with E0428.
#[test]
fn names_differing_only_in_non_ascii_characters_stay_distinct() {
    let mut program = compile(
        "fn f1(x) { return x + 1; } fn f2(x) { return x + 20; } fn f3() { return 300; } fn f4(x) { return x * 1000; } \
         fn main() { return f1(0) + f2(0) + f3() + f4(4); }",
    );
    rename_functions(&mut program, &[("f1", "café"), ("f2", "cafè"), ("f3", "π"), ("f4", "_")]);
    for (backend, so) in build_both(&program, "unicode_names") {
        let result = Module::load(&so).expect("load").call_main().expect("call_main");
        assert_eq!(result, RuntimeValue::Int(4321), "{backend}");
    }
}

/// `return a + b` for two matrices, with the `add` swapped for `mat_mul` (no Crush
/// syntax emits `mat_mul`; hand-written CASM does). C only: the Rust backend has no
/// `mat_mul`.
fn mat_mul_program(a: &str, b: &str, read: &str) -> casm::Program {
    let mut program = compile(&format!("fn main() {{ let a = {a}; let b = {b}; let r = a + b; return {read}; }}"));
    let main = program.functions.get_mut("main").unwrap();
    let add = main.body.iter_mut().find(|i| i.op == "add").expect("add instruction");
    add.op = "mat_mul".to_string();
    program
}

#[test]
fn c_mat_mul_multiplies_and_rejects_mismatched_shapes() {
    if !cc_available() {
        eprintln!("gcc not found; skipping");
        return;
    }
    let compiler = AotCompiler::new();

    let ok = mat_mul_program("[[1, 2], [3, 4]]", "[[5, 6], [7, 8]]", "r[1][1]");
    let so = compiler.compile_c(&ok, "mat_mul_ok", "gcc").expect("c backend");
    assert_eq!(Module::load(&so).unwrap().call_main().unwrap(), RuntimeValue::Float(50.0));

    // Used to read past the end of `b`'s rows (and through a NULL data pointer for the
    // empty one): a segfault or heap garbage.
    for (name, a, b) in [
        ("mat_mul_ragged_b", "[[1, 2, 3]]", "[[1], []]"),
        ("mat_mul_short_b", "[[1, 2, 3]]", "[[1], [2]]"),
        ("mat_mul_ragged_a", "[[1, 2], [3]]", "[[1], [2]]"),
    ] {
        let so = compiler.compile_c(&mat_mul_program(a, b, "len(r)"), name, "gcc").expect("c backend");
        let out = run(&so);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{name}: {stderr}");
        assert!(stderr.contains("mat_mul: shape mismatch"), "{name}: {stderr}");
    }
}
