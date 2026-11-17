//! #446 / CRUSH-466: an explicit `arr_get(a, i)` call has the same contract as
//! the `index` op (`a[i]`) on the scheduler VM and PortableVm: a negative
//! index counts from the end, out of range raises `ArrayBounds`, a non-Int
//! index raises `BadIndex`.
//!
//! The capability used to cast the index with `Value::Int(i) => *i as usize,
//! _ => 0` and return `null` when the read missed: `arr_get(a, -1)` was
//! `null`, `arr_get(a, "x")` read slot 0 and `arr_get(a, 9)` was `null`.
//!
//! FastVM lowers `arr_get` to its `index` op (`FastOp::Index`), so it already
//! counts negatives from the end; its out-of-range and non-Int reads are #445
//! (see `fastvm_*` below).

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
        // return scalars.
        Some(RuntimeValue::Ref(_)) => "ref".into(),
    }
}

/// Run `source` on the scheduler VM, PortableVm and FastVM.
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

fn val(s: &str) -> Out {
    Out::Val(s.into())
}

fn err(s: &str) -> Out {
    Out::Err(s.into())
}

/// `arr_get([1, 2, 3], index)`, behind `any` parameters so the frontend
/// cannot fold or reject it.
fn arr_get(index: &str) -> String {
    format!(
        "fn get(c: any, i: any) {{ return arr_get(c, i); }}\nfn main() {{ return get([1, 2, 3], {index}); }}"
    )
}

/// The same read as the `index` op.
fn index_op(index: &str) -> String {
    format!(
        "fn get(c: any, i: any) {{ return c[i]; }}\nfn main() {{ return get([1, 2, 3], {index}); }}"
    )
}

/// The `arr_get` cases of CRUSH-466 and what the `index` op gives for each.
const CASES: &[(&str, Result<&str, &str>)] = &[
    ("0", Ok("int:1")),
    ("2", Ok("int:3")),
    ("-1", Ok("int:3")),
    ("-3", Ok("int:1")),
    ("-4", Err("array index out of range: -4 (len 3)")),
    ("3", Err("array index out of range: 3 (len 3)")),
    ("9", Err("array index out of range: 9 (len 3)")),
    ("\"x\"", Err("array index must be int, got str")),
    ("1.5", Err("array index must be int, got float")),
    ("true", Err("array index must be int, got bool")),
    ("null", Err("array index must be int, got null")),
];

fn want(case: Result<&str, &str>) -> Out {
    match case {
        Ok(v) => val(v),
        Err(e) => err(e),
    }
}

/// The scheduler VM and PortableVm give the same result for `arr_get(a, i)`
/// as for `a[i]`, value or error text.
#[test]
fn vms_arr_get_matches_the_index_op() {
    for &(index, case) in CASES {
        let want = want(case);
        let [s, p, _] = run3(&arr_get(index));
        assert_eq!(s, want, "scheduler: arr_get(a, {index})");
        assert_eq!(p, want, "PortableVm: arr_get(a, {index})");
        // The `index` op is the reference contract; pin it too so the two
        // cannot drift apart unnoticed.
        let [s, p, _] = run3(&index_op(index));
        assert_eq!(s, want, "scheduler: a[{index}]");
        assert_eq!(p, want, "PortableVm: a[{index}]");
    }
}

/// The loose differential harness sees no scheduler-vs-PortableVm divergence
/// for any case.
#[test]
fn differential_harness_sees_no_vm_divergence() {
    for &(index, _) in CASES {
        let report = differential_run(&arr_get(index)).expect("compiles");
        let vm_pair: Vec<_> = report
            .divergences
            .iter()
            .filter(|d| d.contains("interpreter vs portable"))
            .collect();
        assert!(vm_pair.is_empty(), "arr_get(a, {index}): {vm_pair:?}");
    }
}

/// An `arr_get` error is catchable on both VMs, with the `index` op's message.
#[test]
fn vms_arr_get_errors_are_catchable() {
    for (index, msg) in [
        ("9", "array index out of range: 9 (len 3)"),
        ("-4", "array index out of range: -4 (len 3)"),
        ("\"x\"", "array index must be int, got str"),
    ] {
        let src = format!(
            "fn get(c: any, i: any) {{ return arr_get(c, i); }}\n\
             fn main() {{\n\
               let msg = \"not caught\";\n\
               try {{ get([1, 2, 3], {index}); }} catch e {{ msg = \"\" + e; }}\n\
               return msg == \"error({msg})\";\n\
             }}"
        );
        let [s, p, _] = run3(&src);
        assert_eq!(s, val("bool:true"), "scheduler: try arr_get(a, {index})");
        assert_eq!(p, val("bool:true"), "PortableVm: try arr_get(a, {index})");
    }
}

/// An `arr_get` on a map still reads by key (unchanged).
#[test]
fn vms_arr_get_on_a_map_reads_by_key() {
    let src = "fn get(c: any, i: any) { return arr_get(c, i); }\n\
               fn main() { let m = {\"k\": 7}; return get(m, \"k\"); }";
    let [s, p, _] = run3(src);
    assert_eq!(s, val("int:7"), "scheduler");
    assert_eq!(p, val("int:7"), "PortableVm");
}

/// FastVM's `arr_get` is its `index` op, so in-range reads (negative ones
/// included) already agree with the VMs.
#[test]
fn fastvm_arr_get_in_range_agrees() {
    for &(index, case) in CASES.iter().filter(|(_, c)| c.is_ok()) {
        let [_, _, f] = run3(&arr_get(index));
        assert_eq!(f, want(case), "FastVM: arr_get(a, {index})");
    }
}

/// FastVM's out-of-range and non-Int `arr_get` reads give `null` until #445
/// (FastVM index reads raise) lands; un-ignore this then.
#[test]
#[ignore = "#445: FastVM index reads out of range / non-Int return null"]
fn fastvm_arr_get_errors_agree() {
    for &(index, case) in CASES.iter().filter(|(_, c)| c.is_err()) {
        let [_, _, f] = run3(&arr_get(index));
        assert_eq!(f, want(case), "FastVM: arr_get(a, {index})");
    }
}
