//! #445 / CRUSH-465: an index *read* (`a[i]`, `s[i]`, and `arr_get`, which
//! FastVM lowers to the same `FastOp::Index`) gives the same result on all
//! three Tier-1 engines — scheduler VM, PortableVm and FastVM — including the
//! same error text, and the error is catchable on each of them.
//!
//! FastVM used to push `null` for an out-of-range index, a non-Int index and a
//! non-container receiver, where the two CVM1 engines raise `ArrayBounds`,
//! `BadIndex` and `TypeError`.
//!
//! Each case runs through the existing loose harness
//! (`differential::differential_run`, which checks accept-vs-reject across the
//! three engines) and then through a strict per-engine comparison of the
//! returned scalar or the error's display text.

use crush_lang_sdk::differential::differential_run;
use crush_vm::fastvm::FastYield;
use crush_vm::vm::Value;
use crush_vm::{PortableVm, Quotas, RuntimeValue};

/// One engine's observable outcome: the scalar `main` returned, or the
/// error's display text.
#[derive(Debug, Clone, PartialEq)]
enum Out {
    Val(String),
    Err(String),
}

fn norm_value(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => "null".into(),
        Some(Value::Bool(b)) => format!("bool:{b}"),
        Some(Value::Int(i)) => format!("int:{i}"),
        Some(Value::Float(f)) => format!("float:{f:?}"),
        Some(Value::Str(s)) => format!("str:{s}"),
        Some(other) => format!("other:{other:?}"),
    }
}

fn norm_rtv(v: Option<&RuntimeValue>) -> String {
    match v {
        None | Some(RuntimeValue::Null) => "null".into(),
        Some(RuntimeValue::Bool(b)) => format!("bool:{b}"),
        Some(RuntimeValue::Int(i)) => format!("int:{i}"),
        Some(RuntimeValue::Float(f)) => format!("float:{f:?}"),
        Some(RuntimeValue::String(s)) => format!("str:{s}"),
        // An arena reference is opaque outside the VM; the cases below only
        // return scalars (string results are compared inside the program).
        Some(RuntimeValue::Ref(_)) => "ref".into(),
    }
}

/// Run `source` on scheduler, PortableVm and FastVM.
fn run3(source: &str) -> [Out; 3] {
    let casm = crush_frontend::compile_crush_source(source)
        .unwrap_or_else(|e| panic!("frontend rejected {source:?}: {e}"));
    let prog = crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm_to_vm");

    // The scheduler reports `main`'s value as `return_value`; PortableVm
    // leaves it on top of the stack.
    let scheduler = match crush_vm::run(&prog, &Quotas::default()) {
        Ok(r) => Out::Val(norm_value(r.return_value.as_ref())),
        Err(e) => Out::Err(e.to_string()),
    };
    let portable = match PortableVm::new(prog.clone()).run() {
        Ok(r) => Out::Val(norm_value(r.return_value.as_ref().or(r.stack.last()))),
        Err(e) => Out::Err(e.to_string()),
    };
    let fastvm = match crush_vm::run_fastvm(&casm) {
        Ok(FastYield::Finished(v)) => Out::Val(norm_rtv(v.as_ref())),
        Ok(FastYield::Value(v)) => Out::Val(norm_rtv(Some(&v))),
        Ok(FastYield::Error(e)) | Err(e) => Out::Err(e.to_string()),
        Ok(other) => Out::Err(format!("unexpected yield: {other:?}")),
    };
    [scheduler, portable, fastvm]
}

/// All three engines give `want`, and the loose harness sees no divergence.
#[track_caller]
fn assert_all(source: &str, want: Out) {
    let [s, p, f] = run3(source);
    assert_eq!(s, want, "scheduler on {source}");
    assert_eq!(p, want, "PortableVm on {source}");
    assert_eq!(f, want, "FastVM on {source}");
    let report = differential_run(source).expect("compiles");
    assert!(
        !report.diverged(),
        "differential_run diverged on {source}: {:?}",
        report.divergences
    );
}

fn val(s: &str) -> Out {
    Out::Val(s.into())
}

fn err(s: &str) -> Out {
    Out::Err(s.into())
}

/// `c[i]` behind `any` parameters, so the frontend cannot fold or reject it.
fn read(container: &str, index: &str) -> String {
    format!(
        "fn get(c: any, i: any) {{ return c[i]; }}\nfn main() {{ return get({container}, {index}); }}"
    )
}

/// The same read inside `try`; `main` returns whether the handler ran with
/// the message `want`. The check happens in the program because a FastVM
/// string result is an opaque arena ref outside the VM. Two spellings are
/// accepted because the caught value's *shape* differs (CRUSH-369, unchanged
/// here): the CVM1 engines bind an error value, so `"" + e` is
/// `error(<message>)`; FastVM binds the message string itself. The message
/// text is the same on all three.
fn caught(container: &str, index: &str, want: &str) -> String {
    format!(
        "fn get(c: any, i: any) {{ return c[i]; }}\n\
         fn main() {{\n\
           let msg = \"not caught\";\n\
           try {{ get({container}, {index}); }} catch e {{ msg = \"\" + e; }}\n\
           return msg == \"error({want})\" || msg == \"{want}\";\n\
         }}"
    )
}

// ── in-range reads (unchanged behaviour, pinned) ────────────────────────────

#[test]
fn in_range_array_reads_agree() {
    assert_all(&read("[1, 2, 3]", "0"), val("int:1"));
    assert_all(&read("[1, 2, 3]", "2"), val("int:3"));
    assert_all(&read("[1, 2, 3]", "-1"), val("int:3"));
    assert_all(&read("[1, 2, 3]", "-3"), val("int:1"));
}

#[test]
fn in_range_string_reads_agree() {
    let src = "fn get(c: any, i: any) { return c[i]; }\n\
               fn main() { return get(\"abc\", 1) == \"b\" && get(\"abc\", -1) == \"c\"; }";
    assert_all(src, val("bool:true"));
}

// ── out of range ────────────────────────────────────────────────────────────

#[test]
fn out_of_range_array_read_raises_bounds_error() {
    assert_all(
        &read("[1, 2, 3]", "5"),
        err("array index out of range: 5 (len 3)"),
    );
    assert_all(
        &read("[1, 2, 3]", "3"),
        err("array index out of range: 3 (len 3)"),
    );
    assert_all(
        &read("[1, 2, 3]", "-4"),
        err("array index out of range: -4 (len 3)"),
    );
    assert_all(&read("[]", "0"), err("array index out of range: 0 (len 0)"));
}

#[test]
fn the_issue_445_repro() {
    assert_all(
        "fn main() { let a = [1, 2, 3]; return a[5]; }",
        err("array index out of range: 5 (len 3)"),
    );
}

#[test]
fn out_of_range_string_read_raises_bounds_error() {
    assert_all(
        &read("\"abc\"", "9"),
        err("array index out of range: 9 (len 3)"),
    );
    assert_all(
        &read("\"abc\"", "-4"),
        err("array index out of range: -4 (len 3)"),
    );
}

// ── non-Int index ───────────────────────────────────────────────────────────

#[test]
fn non_int_array_index_raises_bad_index() {
    assert_all(
        &read("[1, 2, 3]", "\"x\""),
        err("array index must be int, got str"),
    );
    assert_all(
        &read("[1, 2, 3]", "1.5"),
        err("array index must be int, got float"),
    );
    assert_all(
        &read("[1, 2, 3]", "true"),
        err("array index must be int, got bool"),
    );
    assert_all(
        &read("[1, 2, 3]", "null"),
        err("array index must be int, got null"),
    );
}

#[test]
fn non_int_string_index_raises_bad_index() {
    assert_all(
        &read("\"abc\"", "\"x\""),
        err("array index must be int, got str"),
    );
}

// ── non-container receiver ──────────────────────────────────────────────────

#[test]
fn indexing_a_non_container_raises_type_error() {
    assert_all(
        &read("5", "0"),
        err("type error: expected array, map, or string, got int"),
    );
    assert_all(
        &read("null", "0"),
        err("type error: expected array, map, or string, got null"),
    );
    assert_all(
        &read("true", "0"),
        err("type error: expected array, map, or string, got bool"),
    );
}

// ── catchable on every engine, with the same message ────────────────────────

#[test]
fn bounds_error_is_caught_with_the_same_message() {
    assert_all(
        &caught("[1, 2, 3]", "10", "array index out of range: 10 (len 3)"),
        val("bool:true"),
    );
    assert_all(
        &caught("[1, 2, 3]", "-10", "array index out of range: -10 (len 3)"),
        val("bool:true"),
    );
    assert_all(
        &caught("\"abc\"", "10", "array index out of range: 10 (len 3)"),
        val("bool:true"),
    );
}

#[test]
fn bad_index_is_caught_with_the_same_message() {
    assert_all(
        &caught("[1, 2, 3]", "\"x\"", "array index must be int, got str"),
        val("bool:true"),
    );
    assert_all(
        &caught("\"abc\"", "\"x\"", "array index must be int, got str"),
        val("bool:true"),
    );
}

#[test]
fn non_container_type_error_is_caught_with_the_same_message() {
    assert_all(
        &caught(
            "5",
            "0",
            "type error: expected array, map, or string, got int",
        ),
        val("bool:true"),
    );
}

// ── arr_get lowers to the same FastVM op ────────────────────────────────────

/// FastVM lowers an explicit `arr_get` call to `FastOp::Index`, so the fix
/// reaches it too. This pins FastVM only: the scheduler/PortableVm `arr_get`
/// capability has its own (different) contract, tracked as #446.
#[test]
fn fastvm_arr_get_takes_the_index_path() {
    let src = "fn main() { let a = [1, 2, 3]; return arr_get(a, 9); }";
    let casm = crush_frontend::compile_crush_source(src).expect("compiles");
    match crush_vm::run_fastvm(&casm) {
        Ok(FastYield::Error(e)) | Err(e) => {
            assert_eq!(e.to_string(), "array index out of range: 9 (len 3)")
        }
        other => panic!("FastVM arr_get(a, 9) should raise, got {other:?}"),
    }
}

// ── writes: #442 made them raise on FastVM; #445 makes that catchable ───────

fn write(container: &str, index: &str) -> String {
    format!(
        "fn set(c: any, i: any) {{ c[i] = 1; return 0; }}\nfn main() {{ return set({container}, {index}); }}"
    )
}

/// FastVM's out-of-range write raised an uncatchable `ExecutionError`
/// (displayed `Execution error: array index out of range: …`) and a non-Int
/// write raised `Type mismatch`; it now raises the same catchable errors as a
/// read, with the CVM1 wording.
#[test]
fn bad_writes_raise_the_read_errors() {
    assert_all(
        &write("[1, 2, 3]", "5"),
        err("array index out of range: 5 (len 3)"),
    );
    assert_all(
        &write("[1, 2, 3]", "\"x\""),
        err("array index must be int, got str"),
    );
    // One line on purpose: a newline before `||` ends the `return` statement.
    let src = "fn set(c: any, i: any) { c[i] = 1; return 0; }\n\
               fn main() {\n\
                 let msg = \"not caught\";\n\
                 try { set([1, 2, 3], 5); } catch e { msg = \"\" + e; }\n\
                 return msg == \"error(array index out of range: 5 (len 3))\" || msg == \"array index out of range: 5 (len 3)\";\n\
               }";
    assert_all(src, val("bool:true"));
}
