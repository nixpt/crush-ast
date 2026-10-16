//! Edge cases where the Rust and C AOT backends must give the same answer (PR #126 review).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crush_aot::{AotCompiler, Module};
use crush_lang_sdk::{Quotas, Runtime};
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

// ── CRUSH-235: method-call arguments reach capabilities in order ────────

/// Run `source` on the VM (it must `print` the answer) and return the parsed Int.
fn run_vm_print(source: &str) -> i64 {
    // Use the SDK's source→VM entry (the same one crush-run routes `.cvm1`
    // blobs through) so capabilities declared by the frontend are honoured.
    let program =
        crush_lang_sdk::compile::compile_crush_source(source).expect("frontend compiles source");
    let quotas = Quotas {
        max_steps: 20_000_000,
        ..Quotas::default()
    };
    let result = Runtime::with_quotas(quotas).run(&program).expect("vm run");
    assert!(result.halted, "program did not halt:\n{}", result.output);
    let first = result.output.lines().next().unwrap_or("");
    first
        .trim()
        .parse::<i64>()
        .expect(&format!("VM: expected Int output from `{source}`, got `{}`", result.output))
}

/// Run `source` on every AOT backend (it must `print` the answer) and return
/// the parsed Int from each.
fn run_aot_print(source: &str) -> Vec<i64> {
    let mut outs = Vec::new();
    for (backend, so) in build_both(&compile(source), "crush_235") {
        let out = run(&so);
        assert!(
            out.status.success(),
            "{backend}: `{source}` failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        outs.push(
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .expect("{backend}: no stdout from `{source}`")
                .trim()
                .parse::<i64>()
                .expect(&format!(
                    "{backend}: expected Int output from `{source}`",
                )),
        );
    }
    outs
}

/// `source` must print `expected` on the VM and on every AOT backend.
fn expect_print_everywhere(source: &str, expected: i64) {
    let vm = run_vm_print(source);
    assert_eq!(vm, expected, "VM: `{source}`");
    for got in run_aot_print(source) {
        assert_eq!(got, expected, "AOT: `{source}`");
    }
}

#[test]
fn arr_slice_method_call_slices_in_order() {
    // CRUSH-235: `xs.arr_slice(1, 3)` must reach arr_slice as (xs, 1, 3),
    // not (xs, 3, 1). Expected [20, 30].
    // arr_slice is a VM-only capability (not lowered by the AOT backends),
    // so this runs on the VM only.
    let vm = run_vm_print("let xs = [10, 20, 30, 40]; let s = xs.arr_slice(1, 3); print(len(s));");
    assert_eq!(vm, 2, "VM: arr_slice len");
    let vm = run_vm_print("let xs = [10, 20, 30, 40]; let s = xs.arr_slice(1, 3); print(s[0]);");
    assert_eq!(vm, 20, "VM: arr_slice[0]");
    let vm = run_vm_print("let xs = [10, 20, 30, 40]; let s = xs.arr_slice(1, 3); print(s[1]);");
    assert_eq!(vm, 30, "VM: arr_slice[1]");
}

#[test]
fn arr_set_method_call_sets_index_in_order() {
    // CRUSH-235: `zs.arr_set(1, 7)` must reach arr_set as (zs, 1, 7), setting
    // index 1 to 7 — not index 7 to 1.
    expect_print_everywhere("let zs = [0, 0, 0, 0]; zs.arr_set(1, 7); print(zs[1]);", 7);
    expect_print_everywhere("let zs = [0, 0, 0, 0]; zs.arr_set(1, 7); print(zs[0]);", 0);
}

#[test]
fn push_method_call_appends_in_order() {
    // `push` takes one value; `xs.push(5)` reaches array_push as (xs, 5).
    expect_print_everywhere("let xs = [1]; xs.push(5); print(len(xs));", 2);
    expect_print_everywhere("let xs = [1]; xs.push(5); print(xs[1]);", 5);
}

// ── CRUSH-zorro-54: a read of an unassigned local fails on every engine ──

/// How one engine ended: `Ok(returned value)` or `Err(error text)`.
type EngineOutcome = Result<String, String>;

/// Run `program`'s `main` on every engine here — scheduler VM, portable VM,
/// FastVM, JIT, AOT Rust and AOT C — and return each one's outcome.
fn run_on_every_engine(program: &casm::Program, name: &str) -> Vec<(&'static str, EngineOutcome)> {
    use crush_vm::fastvm::FastYield;
    let fast = |r: Result<FastYield, String>| match r {
        Ok(FastYield::Finished(v)) => Ok(format!("{:?}", v.unwrap_or(RuntimeValue::Null))),
        Ok(FastYield::Value(v)) => Ok(format!("{v:?}")),
        Ok(FastYield::Error(e)) => Err(format!("{e:?}")),
        Ok(other) => Err(format!("unexpected yield: {other:?}")),
        Err(e) => Err(e),
    };
    let mut out = Vec::new();
    let vm_prog = crush_lang_sdk::compile::casm_to_vm(program).expect("casm_to_vm");
    let stack_top = |r: Result<crush_vm::VmResult, crush_vm::VmError>| match r {
        Ok(res) => Ok(format!("{:?}", res.return_value.as_ref().or(res.stack.last()))),
        Err(e) => Err(e.to_string()),
    };
    out.push(("scheduler vm", stack_top(crush_vm::run(&vm_prog, &Quotas::default()))));
    out.push(("portable vm", stack_top(crush_vm::PortableVm::new(vm_prog.clone()).run())));
    out.push(("fastvm", fast(crush_vm::run_fastvm(program).map_err(|e| format!("{e:?}")))));
    let jit = crush_vm::fastvm::lower_program(program)
        .map_err(|e| format!("lower_program: {e}"))
        .and_then(|lowered| {
            let engine = crush_jit::JitEngine::new().map_err(|e| format!("JitEngine::new: {e}"))?;
            engine.run(&lowered).map_err(|e| format!("{e:#}"))
        });
    out.push(("jit", fast(jit)));
    for (backend, so) in build_both(program, name) {
        let o = run(&so);
        let r = if o.status.success() {
            Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
        } else {
            Err(format!("exit {:?}: {}", o.status.code(), String::from_utf8_lossy(&o.stderr).trim()))
        };
        out.push((if backend == "rust" { "aot rust" } else { "aot c" }, r));
    }
    out
}

/// `program` with the first `store` to `local` in `func` sent to a dead local
/// instead, so `local` is unassigned on every path that only that store
/// assigned. (The frontend refuses to compile a read of an unassigned local,
/// but a walker can emit one — CRUSH-zorro-54.)
fn unassign_first_store(mut program: casm::Program, func: &str, local: &str) -> casm::Program {
    let f = program.functions.get_mut(func).expect("function");
    let store = f
        .body
        .iter_mut()
        .find(|i| i.op == "store" && i.args.get("name").and_then(|n| n.as_str()) == Some(local))
        .expect("a store to the local");
    store.args["name"] = serde_json::Value::from("__dead");
    program
}

const UNINIT: &str = "load from uninitialised slot";

/// Engines that still read an unassigned local as `null`, keyed to the
/// ticket that fixes them. One that turns loud fails the test: remove it.
const STILL_SILENT: &[(&str, &str)] = &[("fastvm", "CRUSH-zorro-55"), ("jit", "CRUSH-zorro-55")];

fn assert_loud_everywhere(program: &casm::Program, name: &str) {
    let outcomes = run_on_every_engine(program, name);
    let report: Vec<String> = outcomes.iter().map(|(e, o)| format!("  {e:<13} {o:?}")).collect();
    assert!(outcomes.len() >= 5, "{name}: too few engines ran:\n{}", report.join("\n"));
    for (engine, o) in &outcomes {
        let loud = matches!(o, Err(e) if e.contains(UNINIT));
        match STILL_SILENT.iter().find(|(e, _)| e == engine) {
            None => assert!(loud, "{name}: {engine} must fail with `{UNINIT}`:\n{}", report.join("\n")),
            Some((_, ticket)) => assert!(
                !loud && matches!(o, Ok(v) if v == "Null"),
                "{name}: {engine} no longer reads null ({ticket}) — remove it from STILL_SILENT:\n{}",
                report.join("\n")
            ),
        }
    }
}

fn assert_value_everywhere(program: &casm::Program, name: &str, vm: &str, aot: &str) {
    let outcomes = run_on_every_engine(program, name);
    let report: Vec<String> = outcomes.iter().map(|(e, o)| format!("  {e:<13} {o:?}")).collect();
    for (engine, o) in &outcomes {
        let want = if engine.starts_with("aot") { aot } else { vm };
        assert!(
            matches!(o, Ok(v) if v.contains(want)),
            "{name}: {engine} must return {want}:\n{}",
            report.join("\n")
        );
    }
}

#[test]
fn unassigned_local_read_in_main_fails_everywhere() {
    // `x` is only ever read: the walker forgot to assign it. (`n(5)`, not
    // `5`, so the frontend can't fold the read away.)
    let program = unassign_first_store(
        compile("fn n(v) { return v; } fn main() { let x = n(5); return x; }"),
        "main",
        "x",
    );
    assert_loud_everywhere(&program, "z54_main");
}

#[test]
fn unassigned_local_read_on_the_untaken_path_fails_everywhere() {
    // `x` is assigned only when `c` is true; `f(false)` reads it unassigned.
    let src = |arg: &str| format!("fn f(c) {{ let x = 0; if c {{ x = 7; }} return x; }} fn main() {{ return f({arg}); }}");
    let program = unassign_first_store(compile(&src("false")), "f", "x");
    assert_loud_everywhere(&program, "z54_cond_untaken");
    // ... and the same function reads it fine when the assigning path ran.
    let program = unassign_first_store(compile(&src("true")), "f", "x");
    assert_value_everywhere(&program, "z54_cond_taken", "7", "int:7");
}

#[test]
fn a_loop_that_assigns_before_reading_still_works() {
    // `y` is first assigned inside the loop body, then read in the same
    // iteration: never unassigned at a read, though no store dominates the
    // loop header.
    let program = unassign_first_store(
        compile("fn main() { let y = 0; let i = 0; let s = 0; while i < 4 { y = i * 2; s = s + y; i = i + 1; } return s; }"),
        "main",
        "y",
    );
    assert_value_everywhere(&program, "z54_loop", "12", "int:12");
}
