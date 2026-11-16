//! #444 / CRUSH-444: `str.split` gives the same result on all three Tier-1
//! engines — scheduler VM, PortableVm and FastVM — for non-string arguments
//! and for the empty delimiter.
//!
//! Ruling (altair, 2026-11-16): parity only. The reference is today's
//! scheduler VM, which renders both arguments through `Display`
//! (`Value::as_text`). Whether string ops should refuse non-strings instead is
//! the captain's open H4; when that lands it changes one place per engine, and
//! these tests change with it.
//!
//! Before this fix FastVM's `StrSplit` raised `TypeMismatch` for any
//! non-string argument and split on `""` with `str::split` (leading and
//! trailing empty parts), and PortableVm rendered through its own
//! `value_to_text` rather than `Display`.
//!
//! `str.split` reaches the engines by two routes, and both are checked:
//!
//! - the **opcode** route (`str_split` in CASM, `STR_SPLIT` / `FastOp::StrSplit`),
//!   which CAST `FunctionCall "str.split"` producers emit (the Python
//!   frontend, the shell walker). All three engines run it. The tests make
//!   it from Crush source by rewriting the one `cap_call "str.split"` into
//!   `str_split`; both take the target then the delimiter.
//! - the **capability** route (`cap_call "str.split"`), which Crush source
//!   `str.split(a, b)` compiles to. The scheduler VM and PortableVm have it
//!   built in; FastVM has no built-in `str.split` capability and needs the
//!   host to supply one (`fastvm_cap_route_needs_a_host_capability` pins
//!   that, unchanged here).
//!
//! Each case is a strict per-engine comparison of the returned array,
//! element by element with each element's type, plus the existing loose
//! harness (`differential::differential_run`, accept-vs-reject) on the
//! capability route.

use std::sync::Arc;

use crush_lang_sdk::differential::differential_run;
use crush_vm::fastvm::{FastVM, FastYield, Hal, lower_program};
use crush_vm::memory::Object;
use crush_vm::vm::Value;
use crush_vm::{PortableVm, Quotas, RuntimeValue};

#[derive(Debug)]
struct NoHal;
impl Hal for NoHal {}

/// One engine's observable outcome: the value `main` returned (an array of
/// strings renders as `["a", "b"]`, each element Debug-quoted so `"1"` and
/// `1` differ), or the error's display text.
#[derive(Debug, Clone, PartialEq)]
enum Out {
    Val(String),
    Err(String),
}

fn norm_value(v: Option<&Value>) -> String {
    match v {
        Some(Value::Array(items)) => {
            let parts: Vec<String> = items
                .borrow()
                .iter()
                .map(|e| match e {
                    Value::Str(s) => format!("{s:?}"),
                    other => format!("non-str:{other:?}"),
                })
                .collect();
            format!("[{}]", parts.join(", "))
        }
        None | Some(Value::Null) => "null".into(),
        Some(other) => format!("other:{other:?}"),
    }
}

fn norm_rtv(v: Option<&RuntimeValue>, vm: &FastVM) -> String {
    let as_str = |e: &RuntimeValue| match e {
        RuntimeValue::String(s) => Some(s.clone()),
        RuntimeValue::Ref(p) => match vm.arena.get(*p) {
            Some(Object::Str(s)) => Some(s.clone()),
            _ => None,
        },
        _ => None,
    };
    match v {
        Some(RuntimeValue::Ref(p)) => match vm.arena.get(*p) {
            Some(Object::Array(items)) => {
                let parts: Vec<String> = items
                    .iter()
                    .map(|e| match as_str(e) {
                        Some(s) => format!("{s:?}"),
                        None => format!("non-str:{e:?}"),
                    })
                    .collect();
                format!("[{}]", parts.join(", "))
            }
            other => format!("other:{other:?}"),
        },
        None | Some(RuntimeValue::Null) => "null".into(),
        Some(other) => format!("other:{other:?}"),
    }
}

/// Which `str.split` the program uses (see the module docs).
#[derive(Clone, Copy, Debug)]
enum Route {
    Opcode,
    Cap,
}

fn compile(source: &str, route: Route) -> casm::Program {
    let mut casm = crush_frontend::compile_crush_source(source)
        .unwrap_or_else(|e| panic!("frontend rejected {source:?}: {e}"));
    if let Route::Opcode = route {
        let mut rewritten = 0;
        for f in casm.functions.values_mut() {
            for ins in &mut f.body {
                if ins.op == "cap_call"
                    && ins.args.get("name").and_then(|n| n.as_str()) == Some("str.split")
                {
                    ins.op = "str_split".into();
                    ins.args = serde_json::json!({});
                    rewritten += 1;
                }
            }
        }
        assert_eq!(
            rewritten, 1,
            "expected one str.split cap_call in {source:?}"
        );
        casm.manifest.permissions.retain(|p| p != "str.split");
    }
    casm
}

/// Run `casm` on scheduler, PortableVm and FastVM.
fn run3(casm: &casm::Program) -> [Out; 3] {
    let prog = crush_lang_sdk::compile::casm_to_vm(casm).expect("casm_to_vm");

    let scheduler = match crush_vm::run(&prog, &Quotas::default()) {
        Ok(r) => Out::Val(norm_value(r.return_value.as_ref())),
        Err(e) => Out::Err(e.to_string()),
    };
    let portable = match PortableVm::new(prog.clone()).run() {
        Ok(r) => Out::Val(norm_value(r.return_value.as_ref().or(r.stack.last()))),
        Err(e) => Out::Err(e.to_string()),
    };
    // Built by hand (not `run_fastvm`) so the arena is still around to read
    // the returned array's strings.
    let lowered = lower_program(casm).expect("lower_program");
    let mut vm = FastVM::new(lowered, vec![], Arc::new(NoHal));
    let fastvm = match vm.run(1_000_000) {
        FastYield::Finished(v) => Out::Val(norm_rtv(v.as_ref(), &vm)),
        FastYield::Value(v) => Out::Val(norm_rtv(Some(&v), &vm)),
        FastYield::Error(e) => Out::Err(e.to_string()),
        other => Out::Err(format!("unexpected yield: {other:?}")),
    };
    [scheduler, portable, fastvm]
}

/// `str.split(a, b)` behind `any` parameters, so the frontend cannot fold or
/// reject it.
fn split(target: &str, delim: &str) -> String {
    format!(
        "fn sp(a: any, b: any) {{ return str.split(a, b); }}\n\
         fn main() {{ return sp({target}, {delim}); }}"
    )
}

/// The engines return the array of strings `want`: all three on the opcode
/// route, scheduler and PortableVm on the capability route, and the loose
/// harness sees no divergence.
#[track_caller]
fn assert_split(target: &str, delim: &str, want: &[&str]) {
    let source = split(target, delim);
    let want = Out::Val(format!(
        "[{}]",
        want.iter()
            .map(|s| format!("{s:?}"))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    let [s, p, f] = run3(&compile(&source, Route::Opcode));
    assert_eq!(s, want, "scheduler STR_SPLIT on split({target}, {delim})");
    assert_eq!(p, want, "PortableVm STR_SPLIT on split({target}, {delim})");
    assert_eq!(f, want, "FastVM StrSplit on split({target}, {delim})");

    let [s, p, _] = run3(&compile(&source, Route::Cap));
    assert_eq!(
        s, want,
        "scheduler cap str.split on split({target}, {delim})"
    );
    assert_eq!(
        p, want,
        "PortableVm cap str.split on split({target}, {delim})"
    );
    let report = differential_run(&source).expect("compiles");
    assert!(
        !report.diverged(),
        "differential_run diverged on split({target}, {delim}): {:?}",
        report.divergences
    );
}

// ── strings (unchanged behaviour, pinned) ───────────────────────────────────

#[test]
fn string_arguments_agree() {
    assert_split("\"a,b,c\"", "\",\"", &["a", "b", "c"]);
    assert_split("\"a,,b\"", "\",\"", &["a", "", "b"]);
    assert_split("\"abc\"", "\"x\"", &["abc"]);
}

// ── non-string target, string delimiter ─────────────────────────────────────

#[test]
fn int_target_agrees() {
    assert_split("1020304", "\"0\"", &["1", "2", "3", "4"]);
    assert_split("-12", "\"-\"", &["", "12"]);
}

#[test]
fn float_target_agrees() {
    assert_split("2.5", "\".\"", &["2", "5"]);
    // A whole float renders with its `.0` on every engine.
    assert_split("3.0", "\".\"", &["3", "0"]);
}

#[test]
fn bool_target_agrees() {
    assert_split("true", "\"u\"", &["tr", "e"]);
    assert_split("false", "\"l\"", &["fa", "se"]);
}

#[test]
fn null_target_agrees() {
    assert_split("null", "\"l\"", &["nu", "", ""]);
}

#[test]
fn array_target_agrees() {
    assert_split("[1, 2.0, true]", "\", \"", &["[1", "2.0", "true]"]);
    assert_split("[\"a\", null]", "\", \"", &["[a", "null]"]);
}

// ── string target, non-string delimiter ─────────────────────────────────────

#[test]
fn int_delimiter_agrees() {
    assert_split("\"a0b0c\"", "0", &["a", "b", "c"]);
}

#[test]
fn float_delimiter_agrees() {
    assert_split("\"x1.5y\"", "1.5", &["x", "y"]);
    assert_split("\"x2.0y\"", "2.0", &["x", "y"]);
}

#[test]
fn bool_delimiter_agrees() {
    assert_split("\"atrueb\"", "true", &["a", "b"]);
}

#[test]
fn null_delimiter_agrees() {
    assert_split("\"anullb\"", "null", &["a", "b"]);
}

#[test]
fn array_delimiter_agrees() {
    assert_split("\"a[1, 2]b\"", "[1, 2]", &["a", "b"]);
}

// ── both non-string (the #444 example) ──────────────────────────────────────

#[test]
fn int_target_and_int_delimiter_agree() {
    assert_split("1020304", "0", &["1", "2", "3", "4"]);
}

// ── empty delimiter: one part per char ──────────────────────────────────────

#[test]
fn empty_delimiter_splits_into_chars() {
    assert_split("\"abc\"", "\"\"", &["a", "b", "c"]);
    assert_split("\"\"", "\"\"", &[]);
    assert_split("\"héllo\"", "\"\"", &["h", "é", "l", "l", "o"]);
    // A non-string target with the empty delimiter.
    assert_split("120", "\"\"", &["1", "2", "0"]);
}

#[test]
fn empty_target_with_delimiter_is_one_empty_part() {
    assert_split("\"\"", "\",\"", &[""]);
}

// ── FastVM's capability route (unchanged, pinned) ───────────────────────────

/// Crush source `str.split(a, b)` is a `cap_call`; FastVM lowers it to a host
/// capability call and has no built-in `str.split`, so without a host one it
/// fails. Not changed by #444 (parity of the built-in engines' converters);
/// pinned so a later fix shows up here.
#[test]
fn fastvm_cap_route_needs_a_host_capability() {
    let [_, _, f] = run3(&compile(&split("\"a,b\"", "\",\""), Route::Cap));
    assert_eq!(f, Out::Err("Invalid capability index: 0".into()));
}
