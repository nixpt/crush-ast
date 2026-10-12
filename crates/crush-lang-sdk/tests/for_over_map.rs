//! Tests for `for` loops over map keys (PR #150: MAP_KEYS opcode).
//!
//! `for k in {"b": 2, "a": 1}` must iterate the keys in sorted order.
//! Arrays and strings pass through unchanged.

use crush_vm::{Quotas, run};

/// Compile Crush source and run it on CVM1, returning captured `print` output.
fn run_source(source: &str) -> String {
    let casm = crush_frontend::compile_crush_source(source).expect("compile");
    let program = crush_lang_sdk::compile::casm_to_vm(&casm).expect("lower");
    let result = run(&program, &Quotas::default()).expect("run");
    result.output
}

#[test]
fn for_over_map_iterates_sorted_keys() {
    let out = run_source(
        r#"
fn main() {
    for k in {"b": 2, "a": 1} {
        print(k)
    }
}
"#,
    );
    assert_eq!(out.trim(), "a\nb", "map keys must iterate in sorted order");
}

#[test]
fn for_over_map_visits_every_key_once() {
    let out = run_source(
        r#"
fn main() {
    let m = {"x": 10, "y": 20, "z": 30}
    let n = 0
    for k in m {
        n = n + 1
    }
    print(n)
}
"#,
    );
    assert_eq!(out.trim(), "3");
}

#[test]
fn for_over_empty_map_runs_zero_times() {
    let out = run_source(
        r#"
fn main() {
    let n = 0
    for k in {} {
        n = n + 1
    }
    print(n)
}
"#,
    );
    assert_eq!(out.trim(), "0");
}

#[test]
fn for_over_array_still_works() {
    // MAP_KEYS must pass arrays through unchanged (no regression)
    let out = run_source(
        r#"
fn main() {
    let total = 0
    for x in [1, 2, 3] {
        total = total + x
    }
    print(total)
}
"#,
    );
    assert_eq!(out.trim(), "6");
}
