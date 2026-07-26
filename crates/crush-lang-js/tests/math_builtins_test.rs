//! End-to-end regression tests for JS `Math.*` lowering (CRUSH-39).
//!
//! `lower_swc.rs` used to pass the JS-side capitalized name (`"Math.floor"`)
//! straight through to `Expression::Call`, but every consumer dispatches on the
//! lowercase `math.*` name. The call therefore matched no builtin arm, fell into
//! crush-frontend's dotted method-call path (`load Math` + `cap_call floor`),
//! and **silently produced a wrong number with no error** —
//! `docs/benchmarks/compute.js` printed 165 instead of 465.
//!
//! These tests deliberately assert the **numeric result**, not merely that
//! compilation succeeded. A "did it compile" test reproduces the exact blind
//! spot that let this ship: the broken form compiled fine.

use crush_lang_js::js_to_cast;

/// JS source → CAST → CASM → VM, returning the trimmed `io.print` output.
///
/// The `math.*` host capabilities (crush-lang-sdk `stdlib.rs:339-350`) are
/// granted because that is what every `Math.*` name now lowers to — the same
/// shape crush's own parser produces for `math.floor(x)` in native source.
fn run_js(source: &str) -> String {
    let cast = js_to_cast(source, "js").expect("js to cast");
    let casm = crush_frontend::compile_cast(&cast).expect("cast to casm");
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm to vm");
    let quotas = crush_vm::Quotas::default();
    let caps = crush_lang_sdk::HostCapsBuilder::new().stdlib(true).build();
    let result = crush_vm::run_with_caps(&vm, &quotas, Some(&caps)).expect("vm run");
    result.output.trim().to_string()
}

/// Assert the printed output is numerically equal to `expected`.
///
/// Compared as a number rather than a string on purpose: the `math.*` caps
/// return `Value::Float`, which renders as `"50.0"` rather than JS's `"50"`.
/// That float-vs-int rendering divergence is a separate tracked issue; pinning
/// the exact string here would make these tests fail for the wrong reason the
/// day it gets fixed.
fn assert_prints(source: &str, expected: f64) {
    let out = run_js(source);
    let got: f64 = out
        .parse()
        .unwrap_or_else(|e| panic!("output {out:?} is not a number ({e}) for source: {source}"));
    assert!(
        (got - expected).abs() < 1e-9,
        "expected {expected}, got {got} (raw output {out:?}) for source: {source}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Every Math.* name lowers to the matching `math.*` host capability
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn math_floor_returns_the_floor_not_zero() {
    // The pre-CRUSH-39 bug yielded a wrong value here, silently.
    assert_prints("console.log(Math.floor(50.7));", 50.0);
}

#[test]
fn math_floor_of_a_division_expression() {
    // The exact shape docs/benchmarks/compute.js uses: Math.floor(d / 5).
    assert_prints("console.log(Math.floor(250 / 5));", 50.0);
}

#[test]
fn math_ceil_returns_the_ceiling() {
    assert_prints("console.log(Math.ceil(50.2));", 51.0);
}

#[test]
fn math_abs_returns_the_magnitude() {
    assert_prints("console.log(Math.abs(0 - 42));", 42.0);
}

#[test]
fn math_round_rounds_to_nearest() {
    assert_prints("console.log(Math.round(2.6));", 3.0);
}

#[test]
fn math_sqrt_returns_the_square_root() {
    assert_prints("console.log(Math.sqrt(144));", 12.0);
}

#[test]
fn math_pow_raises_to_the_power() {
    assert_prints("console.log(Math.pow(2, 10));", 1024.0);
}

// ─────────────────────────────────────────────────────────────────────────────
// min/max — same path; these two have no opcode counterpart at all
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn math_min_returns_the_smaller_argument() {
    assert_prints("console.log(Math.min(3, 9));", 3.0);
}

#[test]
fn math_max_returns_the_larger_argument() {
    assert_prints("console.log(Math.max(3, 9));", 9.0);
}

// ─────────────────────────────────────────────────────────────────────────────
// sin / cos / tan — CRUSH-69: same CapabilityCall path; were missing from the
// producer arm and silently miscompiled after CRUSH-65.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn math_sin_of_zero_is_zero() {
    assert_prints("console.log(Math.sin(0));", 0.0);
}

#[test]
fn math_cos_of_zero_is_one() {
    assert_prints("console.log(Math.cos(0));", 1.0);
}

#[test]
fn math_tan_of_zero_is_zero() {
    assert_prints("console.log(Math.tan(0));", 0.0);
}

#[test]
fn math_sin_of_pi_over_two_is_one() {
    // Closest portable angle without depending on Math.PI (not mapped yet).
    assert_prints("console.log(Math.sin(1.5707963267948966));", 1.0);
}

// ─────────────────────────────────────────────────────────────────────────────
// The benchmark that surfaced the bug
// ─────────────────────────────────────────────────────────────────────────────

/// `docs/benchmarks/compute.js` must print 465.
///
/// Hand-trace: a=100, b=150, c=450, d=250, e=floor(250/5)=50, f=127, g=254,
/// h=154, i=155, j=465. Before CRUSH-39 `e` came out 0, giving 165.
#[test]
fn compute_benchmark_prints_465() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/benchmarks/compute.js");
    let source = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read the compute.js benchmark at {path}: {e}"));
    assert_prints(&source, 465.0);
}

// ─────────────────────────────────────────────────────────────────────────────
// The mapping itself
// ─────────────────────────────────────────────────────────────────────────────

/// Pin the produced builtin names at the CAST level, so a future edit that
/// re-breaks the case mapping fails here with a precise message rather than
/// only as a wrong number several layers downstream.
#[test]
fn math_names_lower_to_lowercase_builtin_names() {
    let source = "\
let a = Math.floor(1.5);
let b = Math.ceil(1.5);
let c = Math.abs(1.5);
let d = Math.round(1.5);
let e = Math.sqrt(1.5);
let f = Math.pow(1.5, 2);
let g = Math.min(1, 2);
let h = Math.max(1, 2);
let i = Math.sin(0);
let j = Math.cos(0);
let k = Math.tan(0);
";
    let cast = js_to_cast(source, "js").expect("js to cast");
    let dumped = format!("{cast:?}");
    for name in [
        "math.floor",
        "math.ceil",
        "math.abs",
        "math.round",
        "math.sqrt",
        "math.pow",
        "math.min",
        "math.max",
        "math.sin",
        "math.cos",
        "math.tan",
    ] {
        assert!(
            dumped.contains(name),
            "lowered CAST is missing the builtin name {name:?}"
        );
    }
    // No capitalized JS-side name may survive lowering — that is the bug.
    assert!(
        !dumped.contains("Math."),
        "lowered CAST still carries a capitalized \"Math.\" name; the CRUSH-39 \
         case mapping has regressed"
    );
}
