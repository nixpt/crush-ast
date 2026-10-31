//! The `js.*` runtime capabilities on the AOT backends: **exact or loud**.
//!
//! `js.add`, `js.truthy`, `js.loose_eq`, `js.strict_eq` and `js.format` are implemented once
//! in crush-vm (`js_core.rs`, embedded verbatim in the AOT Rust runtime) and
//! mirrored in the AOT C runtime (`js_runtime.c`). The node corpus programs
//! that exercise them must print exactly what node printed (`<name>.out`) on
//! both backends, and what the runtime refuses must fail visibly on both.

use std::path::Path;
use std::process::Command;

use crush_aot::AotCompiler;

/// node corpus programs whose output depends on the `js.*` capabilities.
const PROGRAMS: &[&str] = &[
    "concat_number",
    "template_escapes",
    "logical_values",
    "loose_equality",
    "ops_strict_eq",
    "inspect_array",
    "inspect_object",
];

fn have_gcc() -> bool {
    Command::new("gcc").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

/// (success, stdout without the runner's trailing result line, stderr)
fn compile_and_run(backend: &str, name: &str, source: &str) -> Result<(bool, String, String), String> {
    let cast = crush_lang_js::js_to_cast(source, "js").map_err(|e| format!("walker: {e:#}"))?;
    let casm = crush_frontend::compiler::Compiler::new().compile(cast).map_err(|e| format!("compile: {e:#}"))?;
    let aot = AotCompiler::new();
    let so = if backend == "rust" {
        aot.compile_casm(&casm, &format!("jsrt_{name}_r"))
    } else {
        aot.compile_c(&casm, &format!("jsrt_{name}_c"), "gcc")
    }
    .map_err(|e| format!("aot: {e:#}"))?;
    let out = Command::new(env!("CARGO_BIN_EXE_crush-aot-runner")).arg(&so).output().expect("spawn runner");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let body = text.trim_end_matches('\n');
    let printed = match body.rfind('\n') {
        Some(i) => format!("{}\n", &body[..i]),
        None => String::new(),
    };
    Ok((out.status.success(), printed, String::from_utf8_lossy(&out.stderr).to_string()))
}

fn check_exact(backend: &str) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crush-lang-js/tests/node_corpus");
    let mut problems = Vec::new();
    for name in PROGRAMS {
        let source = std::fs::read_to_string(dir.join(format!("{name}.js"))).unwrap();
        let expected = std::fs::read_to_string(dir.join(format!("{name}.out"))).unwrap();
        match compile_and_run(backend, name, &source) {
            Ok((true, got, _)) if got == expected => {}
            Ok((true, got, _)) => problems.push(format!("{name}: {backend} printed WRONG output:\n  node:  {expected:?}\n  crush: {got:?}")),
            Ok((false, _, err)) => problems.push(format!("{name}: {backend} failed: {}", err.trim())),
            Err(e) => problems.push(format!("{name}: {backend}: {e}")),
        }
    }
    assert!(problems.is_empty(), "{} problem(s) on {backend}:\n{}", problems.len(), problems.join("\n"));
}

/// Programs the JS runtime must refuse at run time, with the reason.
const LOUD: &[(&str, &str, &str)] = &[
    ("add_object", "console.log('a' + [1]);", "ToPrimitive"),
    ("eq_object", "console.log([1] == 1);", "ToPrimitive"),
    ("format_directive", "console.log('%d items', 3);", "format directives"),
];

fn check_loud(backend: &str) {
    let mut problems = Vec::new();
    for (name, source, reason) in LOUD {
        match compile_and_run(backend, name, source) {
            Ok((false, _, err)) if err.contains("not supported") && err.contains(reason) => {}
            Ok((ok, out, err)) => problems.push(format!("{name}: {backend} not loud (success {ok}): {out:?} {err:?}")),
            Err(e) => problems.push(format!("{name}: {backend}: {e}")),
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn rust_backend_is_exact() {
    check_exact("rust");
}

#[test]
fn c_backend_is_exact() {
    if !have_gcc() {
        eprintln!("gcc not found; skipping the C backend");
        return;
    }
    check_exact("c");
}

#[test]
fn unsupported_js_semantics_are_loud_on_both_backends() {
    check_loud("rust");
    if have_gcc() {
        check_loud("c");
    }
}
