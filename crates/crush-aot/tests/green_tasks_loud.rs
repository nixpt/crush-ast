//! issue #161: green tasks (`spawn`/`yield`/`await`) run on the scheduler and
//! the portable VM only. AOT Rust, AOT C and FastVM (and so the JIT, which
//! compiles from the FastVM lowering) must refuse them **by name** — a message
//! naming the construct, not the `make_closure` a `spawn` compiles through, and
//! never silently skipping the task.
//!
//! A `make_closure` by itself (a function value used without `spawn`) stays
//! refused as a function value; see `function_values_loud.rs`.

const SPAWN: &str = "fn worker() {}\nfn main() {\n  spawn worker()\n}\n";
const YIELD: &str = "fn main() {\n  yield\n}\n";
const AWAIT: &str = "fn main() {\n  let h = 1\n  await h\n}\n";

fn casm(source: &str) -> casm::Program {
    crush_frontend::compile_crush_source(source).expect("frontend")
}

#[test]
fn aot_rust_refuses_green_tasks_by_name() {
    for (construct, source) in [("spawn", SPAWN), ("yield", YIELD), ("await", AWAIT)] {
        let e = crush_aot::codegen::gen_rust_source(&casm(source)).expect_err("must not compile");
        assert!(e.ops.iter().any(|o| o.contains(construct)), "{construct}: {:?}", e.ops);
        assert!(!e.ops.iter().any(|o| o.contains("make_closure")), "{construct}: {:?}", e.ops);
    }
}

#[test]
fn aot_c_refuses_green_tasks_by_name() {
    for (construct, source) in [("spawn", SPAWN), ("yield", YIELD), ("await", AWAIT)] {
        let e = crush_aot::codegen_c::gen_c_source(&casm(source)).expect_err("must not compile");
        assert!(e.ops.iter().any(|o| o.contains(construct)), "{construct}: {:?}", e.ops);
        assert!(!e.ops.iter().any(|o| o.contains("make_closure")), "{construct}: {:?}", e.ops);
    }
}

#[test]
fn fastvm_refuses_green_tasks_by_name() {
    for (construct, source) in [("spawn", SPAWN), ("yield", YIELD), ("await", AWAIT)] {
        let e = crush_vm::fastvm::lower_program(&casm(source)).expect_err("must not lower");
        assert!(e.to_string().contains(construct), "{construct}: {e}");
        assert!(!e.to_string().contains("make_closure"), "{construct}: {e}");
    }
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
