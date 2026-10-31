//! CRUSH-251 step 5 + #175: `spawn` takes a function value, and
//! `parallel_map(items, f)` runs `f` on each item in its own share-nothing VM
//! (threads on the native scheduler, sequential on the portable VM), with
//! results, output and the first error in input order. Every program runs on
//! both engines, which must agree.

use crush_vm::portable_vm::PortableVm;
use crush_vm::{Quotas, run};

fn both(source: &str) -> Result<String, String> {
    let casm = crush_frontend::compile_crush_source(source).map_err(|e| format!("compile: {e:#}"))?;
    let program = crush_lang_sdk::compile::casm_to_vm(&casm).expect("lower");
    let native = run(&program, &Quotas::default()).map(|r| r.output).map_err(|e| e.to_string());
    let portable = PortableVm::new(program).run().map(|r| r.output).map_err(|e| e.to_string());
    assert_eq!(portable, native, "portable VM and scheduler disagree");
    native
}

fn ok(source: &str) -> String {
    both(source).unwrap_or_else(|e| panic!("{e}"))
}

// Spawned arguments arrived in reverse order: `spawn w(10, 3)` ran w(3, 10)
// and printed -7, silently.
#[test]
fn spawn_passes_arguments_in_order() {
    assert_eq!(
        ok("fn w(a, b) {\n  return a - b\n}\nfn main() {\n  let h = spawn w(10, 3)\n  yield\n  print(await h)\n}\n"),
        "7\n"
    );
}

#[test]
fn async_functions_get_their_arguments_in_order() {
    assert_eq!(
        ok("async fn sub(a, b) {\n  return a - b\n}\nfn main() {\n  let r = await sub(10, 3)\n  print(r)\n}\n"),
        "7\n"
    );
}

#[test]
fn spawn_a_closure() {
    assert_eq!(
        ok("fn main() {\n  let k = 5\n  let f = |x| x + k\n  let h = spawn f(1)\n  yield\n  print(await h)\n}\n"),
        "6\n"
    );
}

#[test]
fn a_spawned_closure_shares_variables_with_its_function() {
    // Green tasks run in the same VM: a captured variable is shared.
    assert_eq!(
        ok("fn main() {\n  let n = 0\n  let inc = || {\n    n = n + 1\n  }\n  let h = spawn inc()\n  yield\n  await h\n  print(n)\n}\n"),
        "1\n"
    );
}

#[test]
fn spawning_with_the_wrong_number_of_arguments_is_an_error() {
    let e = both("fn main() {\n  let f = |a, b| a + b\n  let h = spawn f(1)\n  yield\n}\n").unwrap_err();
    assert!(e.contains("takes 2 argument(s), got 1"), "{e}");
}

#[test]
fn parallel_map_with_a_lambda_and_a_named_function() {
    assert_eq!(
        ok("fn fib(n) {\n  if n < 2 {\n    return n\n  }\n  return fib(n - 1) + fib(n - 2)\n}\nfn main() {\n  print(parallel_map([1, 2, 3, 4], |n| n * n))\n  print(parallel_map([10, 15, 20], fib))\n}\n"),
        "[1, 4, 9, 16]\n[55, 610, 6765]\n"
    );
}

#[test]
fn parallel_map_reads_captured_values() {
    assert_eq!(
        ok("fn main() {\n  let k = 10\n  let base = [1, 2]\n  print(parallel_map([1, 2, 3], |x| x * k + base[1]))\n}\n"),
        "[12, 22, 32]\n"
    );
}

#[test]
fn parallel_map_output_comes_in_input_order() {
    assert_eq!(
        ok("fn main() {\n  let r = parallel_map([3, 1, 2], |x| {\n    print(x)\n    return x + 100\n  })\n  print(r)\n}\n"),
        "3\n1\n2\n[103, 101, 102]\n"
    );
}

#[test]
fn parallel_map_items_share_nothing() {
    // Each item gets its own copy: pushing onto it changes neither the caller's
    // array nor another item's.
    assert_eq!(
        ok("fn main() {\n  let xs = [[1], [2]]\n  let r = parallel_map(xs, |a| {\n    a.push(9)\n    return len(a)\n  })\n  print(r)\n  print(xs)\n}\n"),
        "[2, 2]\n[[1], [2]]\n"
    );
}

#[test]
fn the_first_error_in_item_order_wins() {
    let e = both("fn main() {\n  print(parallel_map([2, 0, 1, 0], |x| 10 / x))\n}\n").unwrap_err();
    assert!(e.contains("parallel_map: item 1: division by zero"), "{e}");
}

#[test]
fn assigning_a_captured_variable_is_an_error() {
    let e = both("fn main() {\n  let total = 0\n  parallel_map([1, 2], |x| {\n    total = total + x\n    return x\n  })\n  print(total)\n}\n").unwrap_err();
    assert!(e.contains("changed something it captured"), "{e}");
}

#[test]
fn nested_parallel_map() {
    assert_eq!(
        ok("fn main() {\n  print(parallel_map([1, 2], |x| parallel_map([10, 20], |y| x * y)))\n}\n"),
        "[[10, 20], [20, 40]]\n"
    );
}

#[test]
fn parallel_map_needs_a_function_of_one_argument() {
    let e = both("fn main() {\n  print(parallel_map([1], |a, b| a))\n}\n").unwrap_err();
    assert!(e.contains("takes 2 argument(s), got 1"), "{e}");
}

// A write through a closure the function captured (transitively) is lost the
// same way: each worker bumped its own copy of `n` and this printed
// `[2, 3, 4]` / `0` without an error (sequential semantics: `[2, 4, 6]` / 3).
#[test]
fn a_write_through_a_captured_closure_is_an_error() {
    let e = both("fn main() {\n  let n = 0\n  let bump = || {\n    n = n + 1\n    return n\n  }\n  let f = |x| x + bump()\n  print(parallel_map([1, 2, 3], f))\n  print(n)\n}\n").unwrap_err();
    assert!(e.contains("changed something it captured"), "{e}");
}

#[test]
fn changing_a_captured_array_in_place_is_an_error() {
    let e = both("fn main() {\n  let seen = []\n  parallel_map([1, 2], |x| {\n    seen.push(x)\n    return x\n  })\n}\n").unwrap_err();
    assert!(e.contains("changed something it captured"), "{e}");
}

#[test]
fn closures_sharing_a_captured_variable_stay_shared_in_a_worker() {
    // `get` and `set` capture the same `v`; inside one worker they still share
    // it (each item starts from the caller's value, 1).
    assert_eq!(
        ok("fn main() {\n  let v = 1\n  let get = || v\n  let set = |x| {\n    v = x\n  }\n  let pair = [get, set]\n  print(parallel_map([5, 7], |x| {\n    let s = pair[1]\n    let g = pair[0]\n    let before = g()\n    return before\n  }))\n}\n"),
        "[1, 1]\n"
    );
}

#[test]
fn a_closure_capturing_its_own_variable_moves_into_a_worker() {
    // `fact` captures `base` and itself (a cell referring back to itself).
    assert_eq!(
        ok("fn main() {\n  let base = 1\n  let fact = 0\n  fact = |n| {\n    if n <= 1 {\n      return base\n    }\n    return n * fact(n - 1)\n  }\n  print(parallel_map([3, 5], fact))\n}\n"),
        "[6, 120]\n"
    );
}
