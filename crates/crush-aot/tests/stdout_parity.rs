//! What AOT-compiled programs *print*, on the Rust and C backends.
//!
//! `differential_aot.rs` compares only `main`'s return value, so bugs on the print path
//! (garbled strings, float formatting, `io.print`'s stack contract) passed it (CRUSH-216,
//! CRUSH-220). Each expected output below is what `crush-run` prints for the same source.

use std::path::{Path, PathBuf};
use std::process::Command;

use crush_aot::AotCompiler;

fn aot_runner() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_crush-aot-runner"))
}

fn cc_available(cc: &str) -> bool {
    Command::new(cc)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Program stdout, without the type-tagged result line the runner prints last.
fn program_stdout(so: &Path) -> String {
    let out = Command::new(aot_runner())
        .arg(so)
        .output()
        .expect("spawn crush-aot-runner");
    assert!(
        out.status.success(),
        "crush-aot-runner failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).expect("utf-8 stdout");
    let body = text.trim_end_matches('\n');
    match body.rfind('\n') {
        Some(i) => format!("{}\n", &body[..i]),
        None => String::new(),
    }
}

/// Like [`assert_prints`], with the expected text taken from the VM, not a literal.
fn assert_prints_like_vm(name: &str, source: &str) {
    assert_prints(name, source, &vm_stdout(source));
}

fn assert_prints(name: &str, source: &str, expected: &str) {
    let program = crush_frontend::compile_crush_source(source).expect("frontend");
    let compiler = AotCompiler::new();

    let so = compiler
        .compile_casm(&program, &format!("{name}_rust"))
        .expect("rust backend");
    assert_eq!(program_stdout(&so), expected, "rust backend: {name}");

    if cc_available("gcc") {
        let so = compiler
            .compile_c(&program, &format!("{name}_c"), "gcc")
            .expect("c backend");
        assert_eq!(program_stdout(&so), expected, "c backend: {name}");
    } else {
        eprintln!("gcc not found; skipping the C backend for {name}");
    }
}

// CRUSH-216: one 256-byte ring buffer held every C string, so a string stored in a
// variable was overwritten by later string work (`keep7` printed `gh36`).
#[test]
fn stored_string_survives_later_string_work() {
    assert_prints(
        "stored_string",
        r#"fn tag(n) { return "keep" + n; }
           fn main() {
               let keep = tag(7);
               let s = "";
               let i = 0;
               while i < 40 { s = "abcdefgh" + i; i = i + 1; }
               io.print(keep);
               io.print(s);
               return 0;
           }"#,
        "keep7\nabcdefgh39\n",
    );
}

// CRUSH-216: a string past 256 bytes came back truncated (len 32), with no error.
#[test]
fn long_string_is_not_truncated() {
    assert_prints(
        "long_string",
        r#"fn main() {
               let s = "";
               let i = 0;
               while i < 30 { s = s + "0123456789"; i = i + 1; }
               io.print(len(s));
               io.print(s == s + "");
               return 0;
           }"#,
        "300\ntrue\n",
    );
}

// The game_of_life render shape: rows built in nested loops, joined with "\n".
#[test]
fn board_built_in_nested_loops() {
    assert_prints(
        "board",
        r##"fn grid(n) {
               let out = "";
               let r = 0;
               while r < n {
                   let line = "";
                   let c = 0;
                   while c < n {
                       let ch = ".";
                       if c == r { ch = "#"; }
                       line = line + ch;
                       c = c + 1;
                   }
                   out = out + line + "\n";
                   r = r + 1;
               }
               io.print(out);
               return;
           }
           fn main() { grid(4); io.print("x=" + 5); return 0; }"##,
        "#...\n.#..\n..#.\n...#\n\nx=5\n",
    );
}

// CRUSH-216: C `io.print` pushed nothing while the CASM pops its result, so the pop
// took the caller's pending operand: `f() + f()` became a type error.
#[test]
fn print_inside_an_expression_keeps_the_stack() {
    assert_prints(
        "print_stack",
        r#"fn f() { io.print("call"); return 1; }
           fn main() { let a = f() + f(); io.print(a); return 0; }"#,
        "call\ncall\n2\n",
    );
}

// CRUSH-216: C printed floats with `%g` (`1`, `0.3`, `1.5e+10`) and concatenated them
// with `%.15g`; `1e20` was emitted as an integer literal too large for C.
#[test]
fn floats_print_like_the_vm() {
    assert_prints_like_vm(
        "floats",
        r#"fn main() {
               io.print(1.0);
               io.print(0.1 + 0.2);
               io.print(2.0 / 3.0);
               io.print(15000000000.0);
               io.print(100000000000000000000.0);
               io.print(0.0000001);
               io.print(-2.5);
               io.print("x=" + (0.1 + 0.2));
               return 0;
           }"#,
    );
}

// CRUSH-216: `conv.chr` returned one of 16 rotating static buffers, so a stored result
// was overwritten after 16 more calls.
#[test]
fn stored_conv_chr_result_survives() {
    assert_prints(
        "conv_chr",
        r#"fn main() {
               let a = conv.chr(65);
               let z = "";
               let i = 0;
               while i < 20 { z = conv.chr(66 + i); i = i + 1; }
               io.print(a + z);
               return 0;
           }"#,
        "AU\n",
    );
}

// CRUSH-223: the C value stack was 512 slots and dropped pushes past the end, so
// `sum(600)` failed with `+ on non-numeric operands`. (crush-run stops at its 256-deep
// call quota instead; AOT has no call-depth limit yet, CRUSH-219.)
#[test]
fn deep_recursion_keeps_every_stack_value() {
    assert_prints(
        "deep_recursion",
        r#"fn sum(n) { if n == 0 { return 0; } return n + sum(n - 1); }
           fn main() { io.print(sum(600)); return 0; }"#,
        "180300\n",
    );
}

// CRUSH-223: the C array pool held 64 arrays of 65,536 fixed slots; running out pushed
// null or dropped elements without an error.
#[test]
fn many_and_large_arrays() {
    assert_prints(
        "arrays",
        r#"fn mk(i) { return [i, i]; }
           fn main() {
               let t = 0;
               let i = 0;
               while i < 100 { let a = mk(i); t = t + a[1]; i = i + 1; }
               io.print(t);
               let b = [];
               i = 0;
               while i < 2000 { b = b + [i]; i = i + 1; }
               io.print(b[1999]);
               let c = [];
               i = 0;
               while i < 100000 { c.push(i); i = i + 1; }
               io.print(len(c));
               return 0;
           }"#,
        "4950\n1999\n100000\n",
    );
}

// CRUSH-223: the C object pool held 32 maps.
#[test]
fn many_maps() {
    assert_prints(
        "maps",
        r#"fn main() {
               let t = 0;
               let i = 0;
               while i < 100 { let o = {"a": i}; t = t + o.a; i = i + 1; }
               io.print(t);
               return 0;
           }"#,
        "4950\n",
    );
}

// CRUSH-201: `io.write` prints without a newline on both AOT backends — C's
// `printf("%d ", i)` in a loop must stay on one line.
#[test]
fn io_write_has_no_implicit_newline() {
    let cast = crush_lang_c::sdk::c_to_cast(
        r#"#include <stdio.h>
           int main() { for (int i = 0; i < 3; i++) printf("%d ", i); printf("|%c\n", 'x'); return 0; }"#,
        "t.c",
    )
    .expect("c walker");
    let program = crush_frontend::compiler::Compiler::new()
        .compile(cast)
        .expect("compile");
    let compiler = AotCompiler::new();
    let so = compiler
        .compile_casm(&program, "io_write_rust")
        .expect("rust backend");
    assert_eq!(program_stdout(&so), "0 1 2 |x\n", "rust backend");
    if cc_available("gcc") {
        let so = compiler
            .compile_c(&program, "io_write_c", "gcc")
            .expect("c backend");
        assert_eq!(program_stdout(&so), "0 1 2 |x\n", "c backend");
    }
}

// ── CRUSH-294 (GitHub #267): heap values print their value, not the arena ref ──

/// Printed stdout from the scheduler VM (the reference every backend must match).
fn vm_stdout(source: &str) -> String {
    use crush_lang_sdk::{Quotas, Runtime};
    let program = crush_lang_sdk::compile::compile_crush_source(source).expect("frontend");
    let quotas = Quotas {
        max_steps: 50_000_000,
        ..Quotas::default()
    };
    let result = Runtime::with_quotas(quotas).run(&program).expect("vm run");
    result.output
}

/// Printed stdout captured from FastVM via `PrintCap`.
fn fastvm_stdout(source: &str) -> String {
    use std::sync::Arc;
    #[derive(Debug)]
    struct DummyHal;
    impl crush_vm::fastvm::Hal for DummyHal {}
    let program = crush_frontend::compile_crush_source(source).expect("frontend");
    let lowered = crush_vm::fastvm::lower_program(&program).expect("lower_program");
    let cap = Arc::new(crush_vm::io_print::PrintCap::new());
    let caps: Vec<Arc<dyn crush_vm::fastvm::Capability>> = vec![cap.clone()];
    let mut vm = crush_vm::fastvm::FastVM::new(lowered, caps, Arc::new(DummyHal));
    let _ = vm.run(50_000_000);
    cap.output()
}

/// Printed stdout captured from the JIT via `PrintCap`.
fn jit_stdout(source: &str) -> String {
    use std::sync::Arc;
    let program = crush_frontend::compile_crush_source(source).expect("frontend");
    let lowered = crush_vm::fastvm::lower_program(&program).expect("lower_program");
    let cap = Arc::new(crush_vm::io_print::PrintCap::new());
    let engine = crush_jit::JitEngine::new()
        .expect("JitEngine")
        .with_capabilities(vec![cap.clone()]);
    let _ = engine.run(&lowered);
    cap.output()
}

/// `source` must print exactly `expected` on the VM, FastVM, JIT and both AOT
/// backends. (The AOT-only `assert_prints` above could not see the FastVM/JIT
/// print path, which is what CRUSH-294 fixes.)
fn assert_prints_everywhere(name: &str, source: &str, expected: &str) {
    assert_eq!(vm_stdout(source), expected, "vm: {name}");
    assert_eq!(fastvm_stdout(source), expected, "fastvm: {name}");
    assert_eq!(jit_stdout(source), expected, "jit: {name}");
    assert_prints(name, source, expected);
}

// CRUSH-294 / GitHub #267: FastVM and the JIT printed `@N` — the arena index —
// for any non-string heap value, and finished successfully. Every engine must
// print the value itself, formatted exactly like the VM.
#[test]
fn heap_values_print_their_value_not_the_arena_ref() {
    assert_prints_everywhere(
        "p_array",
        r#"fn main() { io.print([1, 2, 3]); return 0; }"#,
        "[1, 2, 3]\n",
    );
    assert_prints_everywhere(
        "p_map",
        r#"fn main() { io.print({a: 1}); return 0; }"#,
        "{a: 1}\n",
    );
    assert_prints_everywhere(
        "p_nested",
        r#"fn main() { let xs = [[1], [2, 3]]; io.print(xs); return 0; }"#,
        "[[1], [2, 3]]\n",
    );
    assert_prints_everywhere(
        "p_struct",
        r#"struct P { x: int } fn main() { io.print(P { x: 5 }); return 0; }"#,
        "{x: 5}\n",
    );
    assert_prints_everywhere(
        "p_scalar",
        r#"fn main() { io.print(42); return 0; }"#,
        "42\n",
    );
}

// CRUSH-391: the `pop` capability (the `a.pop()` method form) agrees with the
// VM on every engine: the removed element is the expression's value, an empty
// array pops null, and nothing is left on the stack under it. (str.ord /
// str.chr stay out of the every-engine source: the FastVM/JIT only know the
// capabilities their caller supplies — there the conv.* pair is the shared
// implementation, and the AOT backends inline both names identically.)
#[test]
fn method_pop_matches_the_vm_on_every_engine() {
    assert_prints_everywhere(
        "method_pop",
        r#"fn main() {
               let a = [10, 20, 30]
               let x = a.pop()
               io.print(x)
               io.print(len(a))
               io.print(a.pop())
               let empty = []
               io.print(empty.pop())
               return 0
           }"#,
        "30\n2\n20\nnull\n",
    );
}

/// Both AOT backends fail the run (non-zero exit, `needle` on stderr), as the
/// VM raises an error, instead of printing a value.
fn assert_fails_on_aot(name: &str, source: &str, needle: &str) {
    let program = crush_frontend::compile_crush_source(source).expect("frontend");
    let compiler = AotCompiler::new();
    let mut backends = vec![(
        "rust",
        compiler
            .compile_casm(&program, &format!("{name}_rust"))
            .expect("rust backend"),
    )];
    if cc_available("gcc") {
        backends.push((
            "c",
            compiler
                .compile_c(&program, &format!("{name}_c"), "gcc")
                .expect("c backend"),
        ));
    }
    for (backend, so) in backends {
        let out = Command::new(aot_runner())
            .arg(&so)
            .output()
            .expect("spawn crush-aot-runner");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{backend} backend: {name} must fail");
        assert!(
            stderr.contains(needle),
            "{backend} backend: {name}: {stderr}"
        );
    }
}

// CRUSH-391 review: `pop` on a non-array is an error on the VM; the AOT
// backends used to print null and exit 0.
#[test]
fn method_pop_on_a_non_array_fails_on_the_aot_backends() {
    let source = r#"fn main() { let n = 5
               io.print(n.pop())
               return 0 }"#;
    let program = crush_lang_sdk::compile::compile_crush_source(source).expect("frontend");
    let vm = crush_lang_sdk::Runtime::with_quotas(crush_lang_sdk::Quotas::default()).run(&program);
    assert!(vm.is_err(), "the VM refuses pop on a non-array");
    assert_fails_on_aot("pop_non_array", source, "pop");
}

// CRUSH-391: the str.* names route to the same conv.* implementation on both
// AOT backends (inlined identically), so alias and original agree exactly.
#[test]
fn str_ord_chr_aliases_match_conv_on_the_aot_backends() {
    assert_prints(
        "str_ord_chr_aliases",
        r#"fn main() {
               io.print(str.ord("A"))
               io.print(str.chr(98))
               io.print(conv.ord("A"))
               io.print(conv.chr(98))
               return 0
           }"#,
        "65\nb\n65\nb\n",
    );
}

// #559 review 1: an array passed as a function parameter (`q.crush`) lowers
// uniformly to `call_member`, and must still run on the VMs, FastVM, JIT, and
// both AOT backends (`crush-aotc` CLI included).
#[test]
fn param_array_call_member_runs_on_vm_fastvm_jit_and_aot() {
    let q_source = r#"fn add(xs) {
  xs.push(3)
  return xs.pop()
}
fn main() { return add([1, 2]) }
"#;
    let casm = crush_frontend::compile_crush_source(q_source).expect("frontend");
    let prog = crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm_to_vm");

    // 1. Scheduler VM + PortableVm
    let sched = crush_vm::run(&prog, &crush_vm::Quotas::default()).expect("scheduler VM");
    assert_eq!(sched.return_value, Some(crush_vm::vm::Value::Int(3)));
    let portable = crush_vm::PortableVm::new(prog).run().expect("PortableVm");
    assert_eq!(
        portable.return_value.or(portable.stack.last().cloned()),
        Some(crush_vm::vm::Value::Int(3))
    );

    // 2. FastVM
    let fast = crush_vm::run_fastvm(&casm).expect("FastVM");
    assert_eq!(
        fast,
        crush_vm::fastvm::FastYield::Finished(Some(crush_vm::RuntimeValue::Int(3)))
    );

    // 3. JIT
    let lowered = crush_vm::fastvm::lower_program(&casm).expect("lower for JIT");
    let jit = crush_jit::JitEngine::new().expect("JitEngine");
    let jit_res = jit.run(&lowered).expect("JIT run");
    assert_eq!(
        jit_res,
        crush_vm::fastvm::FastYield::Finished(Some(crush_vm::RuntimeValue::Int(3)))
    );

    // 4. Rust AOT + C AOT via AotCompiler & crush-aot-runner
    let compiler = AotCompiler::new();
    let rust_so = compiler
        .compile_casm(&casm, "q_param_array_rust")
        .expect("rust backend");
    let rust_out = Command::new(aot_runner())
        .arg(&rust_so)
        .output()
        .expect("spawn crush-aot-runner");
    assert!(
        rust_out.status.success(),
        "rust AOT failed: {}",
        String::from_utf8_lossy(&rust_out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&rust_out.stdout).trim(), "int:3");

    if cc_available("gcc") {
        let c_so = compiler
            .compile_c(&casm, "q_param_array_c", "gcc")
            .expect("c backend");
        let c_out = Command::new(aot_runner())
            .arg(&c_so)
            .output()
            .expect("spawn crush-aot-runner");
        assert!(
            c_out.status.success(),
            "C AOT failed: {}",
            String::from_utf8_lossy(&c_out.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&c_out.stdout).trim(), "int:3");
    }

    // 5. `crush-aotc run q.crush` CLI end-to-end
    let dir = tempfile::tempdir().expect("tempdir");
    let q_path = dir.path().join("q.crush");
    std::fs::write(&q_path, q_source).expect("write q.crush");
    let cli = Command::new(env!("CARGO_BIN_EXE_crush-aotc"))
        .args(["run", q_path.to_str().unwrap()])
        .output()
        .expect("spawn crush-aotc");
    assert!(
        cli.status.success(),
        "crush-aotc run q.crush failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&cli.stdout),
        String::from_utf8_lossy(&cli.stderr)
    );
    assert!(
        String::from_utf8_lossy(&cli.stdout).contains("3"),
        "expected 3 in crush-aotc stdout, got: {}",
        String::from_utf8_lossy(&cli.stdout)
    );

    // 6. All built-in array member ops through a parameter receiver on VM + AOT.
    assert_prints_everywhere(
        "param_array_all_builtins",
        r#"fn ops(xs) {
               xs.append(30)
               xs.arr_set(0, 11)
               io.print(xs.arr_get(0))
               io.print(xs.arr_get(-1))
               let sl = xs.arr_slice(1, 3)
               io.print(len(sl))
               io.print(sl.arr_get(0))
               return 0
           }
           fn main() { return ops([10, 20]) }"#,
        "11\n30\n2\n20\n",
    );
}

// #559: `m.arr_get("k")` / `m.arr_set("k", 2)` on a map (with or without an
// unrelated struct declaring the method name) compile through the array
// fallback on AOT and refuse loudly at run time instead of indexing the map.
#[test]
fn map_arr_get_and_arr_set_fail_loudly_at_runtime_on_aot() {
    assert_fails_on_aot(
        "map_arr_get_refusal_no_struct",
        "let m = {\"k\": 1}\nio.print(m.arr_get(\"k\"))\n",
        "arr_get",
    );
    assert_fails_on_aot(
        "map_arr_get_refusal_with_struct",
        "struct S { x }\nimpl S { fn arr_get(self, k) { return 0 } }\nlet m = {\"k\": 1}\nio.print(m.arr_get(\"k\"))\n",
        "arr_get",
    );
    assert_fails_on_aot(
        "map_arr_set_refusal_no_struct",
        "let m = {\"k\": 1}\nm.arr_set(\"k\", 2)\n",
        "arr_set",
    );
    assert_fails_on_aot(
        "map_arr_set_refusal_with_struct",
        "struct S { x }\nimpl S { fn arr_set(self, k, v) { return 0 } }\nlet m = {\"k\": 1}\nm.arr_set(\"k\", 2)\n",
        "arr_set",
    );
}
