//! issue #161: green tasks run on the scheduler and the portable VM only.
//! `spawn` is refused **by name** on every engine without a green-task
//! scheduler (AOT Rust, AOT C and FastVM — and so the JIT, which compiles from
//! the FastVM lowering) — a message naming `spawn`, not the `make_closure` a
//! `spawn` compiles through, and never silently skipping the task.
//!
//! `yield` and `await` without `spawn` are **not** green-task constructs: with
//! no spawned task `yield` is a no-op and `await` has no handle (`await v` on a
//! non-handle returns `v`, issue #251), so every engine lowers them to a no-op
//! rather than refusing. See `differential_aot.rs` for the lone-`yield` and
//! non-handle-`await` agreement.
//!
//! A `make_closure` by itself (a function value used without `spawn`) stays
//! refused as a function value; see `function_values_loud.rs`.

const SPAWN: &str = "fn worker() {}\nfn main() {\n  spawn worker()\n}\n";

fn casm(source: &str) -> casm::Program {
    crush_frontend::compile_crush_source(source).expect("frontend")
}

#[test]
fn aot_rust_refuses_spawn_by_name() {
    let e = crush_aot::codegen::gen_rust_source(&casm(SPAWN)).expect_err("must not compile");
    assert!(e.ops.iter().any(|o| o.contains("spawn")), "{:?}", e.ops);
    assert!(!e.ops.iter().any(|o| o.contains("make_closure")), "{:?}", e.ops);
}

#[test]
fn aot_c_refuses_spawn_by_name() {
    let e = crush_aot::codegen_c::gen_c_source(&casm(SPAWN)).expect_err("must not compile");
    assert!(e.ops.iter().any(|o| o.contains("spawn")), "{:?}", e.ops);
    assert!(!e.ops.iter().any(|o| o.contains("make_closure")), "{:?}", e.ops);
}

#[test]
fn fastvm_refuses_spawn_by_name() {
    let e = crush_vm::fastvm::lower_program(&casm(SPAWN)).expect_err("must not lower");
    assert!(e.to_string().contains("spawn"), "{e}");
    assert!(!e.to_string().contains("make_closure"), "{e}");
}

/// `yield` alone is a no-op, not a refusal: it compiles/lowers on every engine.
#[test]
fn yield_alone_is_not_loud() {
    let source = "fn main() {\n  yield\n}\n";
    crush_aot::codegen::gen_rust_source(&casm(source)).expect("rust: yield alone is a no-op");
    crush_aot::codegen_c::gen_c_source(&casm(source)).expect("c: yield alone is a no-op");
    crush_vm::fastvm::lower_program(&casm(source)).expect("fastvm: yield alone is a no-op");
}

/// `await` alone has no handle, so it is a no-op (returns the value), not a
/// refusal.
#[test]
fn await_alone_is_not_loud() {
    let source = "fn main() {\n  let h = 1\n  await h\n}\n";
    crush_aot::codegen::gen_rust_source(&casm(source)).expect("rust: await alone is a no-op");
    crush_aot::codegen_c::gen_c_source(&casm(source)).expect("c: await alone is a no-op");
    crush_vm::fastvm::lower_program(&casm(source)).expect("fastvm: await alone is a no-op");
}

/// A function value used without `spawn` is a distinct feature (CRUSH-251) and
/// must still be refused as `make_closure` — not re-reported as `spawn`.
#[test]
fn a_closure_value_without_spawn_is_not_reported_as_spawn() {
    let source = "fn apply(f, v) {\n  return f(v)\n}\nfn main() {\n  print(apply(|x| x * 2, 21))\n}\n";
    let e = crush_aot::codegen::gen_rust_source(&casm(source)).expect_err("must not compile");
    assert!(e.ops.iter().any(|o| o.contains("make_closure")), "{:?}", e.ops);
    assert!(!e.ops.iter().any(|o| o.contains("spawn")), "{:?}", e.ops);
}
