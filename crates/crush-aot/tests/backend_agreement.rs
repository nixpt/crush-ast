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
            let r = engine.run(&lowered).map_err(|e| format!("{e:#}"))?;
            // `run` falls back to FastVM when it can't compile: that wouldn't
            // be measuring the JIT.
            if engine.compilation_count() == 0 {
                return Err("JIT fell back to FastVM".to_string());
            }
            Ok(r)
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
const STILL_SILENT: &[(&str, &str)] = &[];

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

// ── CRUSH-zorro-55: the same cases on FastVM and the JIT, plus frame reuse ──

#[test]
fn a_function_local_read_before_its_let_fails_everywhere() {
    // `x` is read before the only store left to it, in a callee. (`n(0)`,
    // not `0`, so the frontend can't fold the read away.)
    let program = unassign_first_store(
        compile("fn n(v) { return v; } fn f(v) { let x = n(0); let r = x + v; x = 5; return r + x; } fn main() { return f(1); }"),
        "f",
        "x",
    );
    assert_loud_everywhere(&program, "z55_before_let");
}

#[test]
fn a_reused_frame_does_not_leak_the_previous_calls_local() {
    // The first `g(true)` assigns `x` in the callee's frame; the second call
    // reuses that frame (same depth) and must not read the old 7.
    let program = unassign_first_store(
        compile("fn g(c) { let x = 0; if c { x = 7; } return x; } fn main() { let a = g(true); let b = g(false); return a + b; }"),
        "g",
        "x",
    );
    assert_loud_everywhere(&program, "z55_reused_frame");
}

#[test]
fn recursion_does_not_leak_a_deeper_frames_local() {
    // `r(3, true)` assigns `x` at every depth; `r(3, false)` then reuses
    // those frames and reads `x` unassigned at the deepest one.
    let src = |second: &str| {
        format!(
            "fn r(n, c) {{ let x = 0; if c {{ x = n + 10; }} if n > 0 {{ let s = r(n - 1, c); return x + s; }} return x; }} \
             fn main() {{ let a = r(3, true); let b = r(3, {second}); return a + b; }}"
        )
    };
    let program = unassign_first_store(compile(&src("false")), "r", "x");
    assert_loud_everywhere(&program, "z55_recursion");
    // ... and every engine agrees when each depth assigns it: 2 * (13+12+11+10).
    let program = unassign_first_store(compile(&src("true")), "r", "x");
    assert_value_everywhere(&program, "z55_recursion_ok", "92", "int:92");
}

// ── #223: `==` / `!=` on maps and arrays must compare contents, not identity ──

/// The full printed output from the VM (scheduler tier). The portable VM shares
/// `crush_vm::Value::PartialEq` (the equality this test pins), so it agrees by
/// construction; the bug being fixed here is C-only.
fn vm_output(source: &str) -> String {
    let program =
        crush_lang_sdk::compile::compile_crush_source(source).expect("frontend compiles source");
    let quotas = Quotas {
        max_steps: 20_000_000,
        ..Quotas::default()
    };
    let result = Runtime::with_quotas(quotas).run(&program).expect("vm run");
    assert!(result.halted, "program did not halt:\n{}", result.output);
    result.output
}

/// Program stdout, without the type-tagged result line the runner prints last.
fn program_stdout(so: &Path) -> String {
    let out = run(so);
    assert!(
        out.status.success(),
        "runner failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).expect("utf-8 stdout");
    let body = text.trim_end_matches('\n');
    match body.rfind('\n') {
        Some(i) => format!("{}\n", &body[..i]),
        None => String::new(),
    }
}

/// `source` must print exactly `expected` on the VM and on every AOT backend.
fn expect_print_everywhere_str(source: &str, expected: &str) {
    assert_eq!(vm_output(source), expected, "VM printed wrong for {source:?}");
    for (backend, so) in build_both(&compile(source), "eq_223") {
        assert_eq!(
            program_stdout(&so),
            expected,
            "{backend} printed wrong for {source:?}"
        );
    }
}

#[test]
fn map_and_array_equality_agrees_across_engines() {
    // (source, expected printed output) — equal/unequal pairs for maps, arrays,
    // nested maps/arrays, mixed map/array, strings, and int-vs-float, plus the
    // two programs from #223.
    let cases: &[(&str, &str)] = &[
        // The two programs from the issue.
        (
            "struct P { x: int, y: int } fn main() { let a = P { x: 1, y: 2 }; let b = P { x: 1, y: 2 }; print(a == b); }",
            "true\n",
        ),
        (
            "struct P { x: int } struct Q { x: int } fn main() { print(P { x: 1 } == Q { x: 1 }); }",
            "true\n",
        ),
        // Maps (struct instances are maps): equal / unequal by value.
        (
            "struct P { x: int, y: int } fn main() { print(P { x: 1, y: 2 } == P { x: 1, y: 2 }); print(P { x: 1, y: 2 } == P { x: 1, y: 3 }); }",
            "true\nfalse\n",
        ),
        // Map equality is order-insensitive (IndexMap equality in the VM).
        (
            "fn main() { print({\"a\": 1, \"b\": 2} == {\"b\": 2, \"a\": 1}); }",
            "true\n",
        ),
        // A missing key is not the same as a key holding null.
        (
            "fn main() { print({\"a\": null} == {\"b\": null}); print({\"a\": null} == {\"a\": 1}); }",
            "false\nfalse\n",
        ),
        // Arrays: length + elementwise.
        (
            "fn main() { print([1, 2, 3] == [1, 2, 3]); print([1, 2] == [1, 3]); print([1, 2] == [1, 2, 3]); }",
            "true\nfalse\nfalse\n",
        ),
        // Nested arrays.
        (
            "fn main() { print([[1, 2], [3]] == [[1, 2], [3]]); print([[1, 2]] == [[1, 3]]); }",
            "true\nfalse\n",
        ),
        // Mixed map/array nesting.
        (
            "fn main() { print({\"a\": [1, 2], \"b\": {\"c\": 3}} == {\"b\": {\"c\": 3}, \"a\": [1, 2]}); print({\"a\": [1, 2]} == {\"a\": [1, 3]}); }",
            "true\nfalse\n",
        ),
        // Arrays nested in maps nested in arrays.
        (
            "fn main() { print([1, [2, {\"k\": 3}]] == [1, [2, {\"k\": 3}]]); print([1, [2, {\"k\": 3}]] == [1, [2, {\"k\": 4}]]); }",
            "true\nfalse\n",
        ),
        // Empty collections and null elements.
        (
            "fn main() { print([] == []); print({} == {}); print([null] == [null]); }",
            "true\ntrue\ntrue\n",
        ),
        // Strings.
        (
            "fn main() { print(\"hi\" == \"hi\"); print(\"hi\" == \"ho\"); }",
            "true\nfalse\n",
        ),
        // int vs float cross-type (and plain ints).
        (
            "fn main() { print(1 == 1.0); print(1 == 2); print(1 != 1.0); }",
            "true\nfalse\nfalse\n",
        ),
        // `!=` on maps and arrays.
        (
            "struct P { x: int } fn main() { print(P { x: 1 } != P { x: 1 }); print(P { x: 1 } != P { x: 2 }); print([1] != [1]); print([1] != [2]); }",
            "false\ntrue\nfalse\ntrue\n",
        ),
    ];
    for (source, expected) in cases {
        expect_print_everywhere_str(source, expected);
    }
}

// ── CRUSH-213: division edge cases must fail loudly on every engine ──

/// `source`'s `main` must *fail* (a runtime error, not a crash) on every
/// engine. When `mentions` is set, every engine's error text must contain it
/// (lower-cased substring).
fn expect_error_everywhere(source: &str, mentions: Option<&str>, name: &str) {
    let program = compile(source);
    let outcomes = run_on_every_engine(&program, name);
    let report: Vec<String> = outcomes.iter().map(|(e, o)| format!("  {e:<13} {o:?}")).collect();
    assert!(outcomes.len() >= 5, "{name}: too few engines ran:\n{}", report.join("\n"));
    for (engine, o) in &outcomes {
        assert!(o.is_err(), "{name}: {engine} must fail for {source:?}:\n{}", report.join("\n"));
        // The JIT refuses some programs it cannot represent exactly (e.g. an
        // int outside its 48-bit payload, like `i64::MIN`) and falls back to
        // FastVM — which still errors. That is a compile refusal, not a wrong
        // result, so it does not have to carry the engine's error text.
        if *engine == "jit" && o.as_ref().unwrap_err().contains("fell back") {
            continue;
        }
        if let Some(m) = mentions {
            let text = o.as_ref().unwrap_err().to_lowercase();
            assert!(
                text.contains(m),
                "{name}: {engine} error {text:?} must mention {m:?}:\n{}",
                report.join("\n")
            );
        }
    }
}

#[test]
fn division_edge_cases_agree_across_every_engine() {
    // `i64::MIN / -1` and `% -1` overflow; `/ 0` and `% 0` are division by
    // zero. AOT C used to SIGFPE, the interp and FastVM panicked in Rust, and
    // the JIT trapped (SIGILL/SIGFPE) or returned `inf` for floats — a crash
    // on some engines and a wrong value on another (CRUSH-213). `id(…)` stops
    // the frontend folding the operands.
    let cases: &[(&str, &str, &str)] = &[
        ("c213_min_div", "overflow",
         r#"fn id(x) { return x; } fn main() { let m = id(0 - 9223372036854775807) - 1; return m / id(-1); }"#),
        ("c213_min_mod", "overflow",
         r#"fn id(x) { return x; } fn main() { let m = id(0 - 9223372036854775807) - 1; return m % id(-1); }"#),
        ("c213_int_div0", "divi",
         r#"fn id(x) { return x; } fn main() { return 10 / id(0); }"#),
        ("c213_int_mod0", "divi",
         r#"fn id(x) { return x; } fn main() { return 10 % id(0); }"#),
        ("c213_float_div0", "divi",
         r#"fn id(x) { return x; } fn main() { return 1.0 / id(0.0); }"#),
        ("c213_float_mod0", "divi",
         r#"fn id(x) { return x; } fn main() { return 1.0 % id(0.0); }"#),
    ];
    for (name, mentions, source) in cases {
        expect_error_everywhere(source, Some(mentions), name);
    }
}

// ── CRUSH-215: EQ/NE must agree on *every* engine, not just VM + AOT ──

/// `source`'s `main` returns a Bool; every engine (scheduler VM, portable VM,
/// FastVM, JIT, AOT Rust, AOT C) must return `expected`.
fn expect_bool_everywhere(source: &str, expected: bool, name: &str) {
    let program = compile(source);
    let outcomes = run_on_every_engine(&program, name);
    let report: Vec<String> = outcomes.iter().map(|(e, o)| format!("  {e:<13} {o:?}")).collect();
    assert!(outcomes.len() >= 5, "{name}: too few engines ran:\n{}", report.join("\n"));
    let want = if expected { "true" } else { "false" };
    for (engine, o) in &outcomes {
        assert!(
            matches!(o, Ok(v) if v.contains(want)),
            "{name}: {engine} must return {want} for {source:?}:\n{}",
            report.join("\n")
        );
    }
}

#[test]
fn equality_agrees_across_every_engine() {
    // CRUSH-215: FastVM fell through to `_ => false` for every non-numeric
    // `==` (so `"ab" == "ab"` was false and `"ab" != "ab"` was false too), its
    // Bool arm computed `x == y` even for `!=`, and the JIT compared strings
    // by NaN-boxed bits so string `==` was always false. Every value kind the
    // engines share must now agree with the interpreter.
    let cases: &[(&str, bool)] = &[
        // Strings (the ticket's headline).
        (r#"fn main() { return "ab" == "ab"; }"#, true),
        (r#"fn main() { return "ab" != "ab"; }"#, false),
        (r#"fn main() { return "ab" == "ba"; }"#, false),
        (r#"fn main() { return "ab" != "ba"; }"#, true),
        // Bool / null via a call, so the frontend cannot fold the comparison.
        (r#"fn id(x) { return x; } fn main() { return id(true) != id(false); }"#, true),
        (r#"fn id(x) { return x; } fn main() { return id(true) == id(true); }"#, true),
        (r#"fn id(x) { return x; } fn main() { return id(null) != id(1); }"#, true),
        (r#"fn id(x) { return x; } fn main() { return id(null) == id(null); }"#, true),
        // Arrays and maps: by value, not identity.
        (r#"fn main() { return [1, 2, 3] == [1, 2, 3]; }"#, true),
        (r#"fn main() { return [1, 2] != [1, 3]; }"#, true),
        (r#"fn main() { return {"a": 1, "b": 2} == {"b": 2, "a": 1}; }"#, true),
        (r#"fn main() { return {"a": 1} == {"a": 2}; }"#, false),
        // Numeric cross-type.
        (r#"fn main() { return 1 == 1.0; }"#, true),
        (r#"fn main() { return 1 != 1.0; }"#, false),
    ];
    for (i, (source, expected)) in cases.iter().enumerate() {
        expect_bool_everywhere(source, *expected, &format!("eq215_{i}"));
    }
}

// ── CRUSH-295 (GitHub #273): a function with more locals than the JIT's
//    per-frame stride must not have them overwritten by a callee ──

#[test]
fn many_locals_survive_a_call_on_every_engine() {
    // `f` uses 11 locals (a..j, k) and calls `g`. With the JIT's old fixed
    // 8-slot frame, g's frame overlapped f's locals 8+ and clobbered them, so
    // the JIT returned a wrong sum while every other engine was correct.
    let src = r#"
        fn g(x) { let p = x + 1; let q = p * 2; let r = q + 3; return r; }
        fn f(n) {
            let a = n + 1; let b = a + 1; let c = b + 1; let d = c + 1; let e = d + 1;
            let f1 = e + 1; let g1 = f1 + 1; let h = g1 + 1; let i = h + 1; let j = i + 1;
            let k = g(a);
            return a + b + c + d + e + f1 + g1 + h + i + j + k;
        }
        fn main() { return f(1); }
    "#;
    assert_value_everywhere(&compile(src), "c295_many_locals", "74", "int:74");
}

#[test]
fn recursion_with_many_locals_agrees_on_every_engine() {
    // Recursive `r` with 10 locals, called to depth 4 — the frame at each
    // depth must keep its own locals.
    let src = r#"
        fn r(n) {
            let a = n; let b = a + 1; let c = b + 1; let d = c + 1; let e = d + 1;
            let f1 = e + 1; let g1 = f1 + 1; let h = g1 + 1; let i = h + 1; let j = i + 1;
            if n == 0 { return a + b + c + d + e + f1 + g1 + h + i + j; }
            return r(n - 1) + j;
        }
        fn main() { return r(3); }
    "#;
    assert_value_everywhere(&compile(src), "c295_recursion", "78", "int:78");
}

// CRUSH-241: `io.eprint` writes to stderr on both AOT backends, like the VM.
#[test]
fn io_eprint_goes_to_stderr_on_both_backends() {
    let program = compile("fn main() { io.print(\"out\"); io.eprint(\"diag \", 1); return 0; }");
    for (backend, so) in build_both(&program, "eprint") {
        let out = run(&so);
        let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(stdout.contains("out\n"), "{backend}: stdout lost io.print: {stdout:?}");
        assert!(!stdout.contains("diag"), "{backend}: io.eprint leaked into stdout: {stdout:?}");
        assert!(stderr.contains("diag 1\n"), "{backend}: io.eprint missing from stderr: {stderr:?}");
    }
}
