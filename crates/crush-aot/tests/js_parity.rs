//! JS e2e parity on the AOT backends: **exact or loud** (CRUSH-zorro-4).
//!
//! Every program in `crush-lang-js/tests/node_corpus/` is compiled (walker →
//! CASM) by the Rust and C backends. A run that succeeds must print exactly
//! what `node` printed (`<name>.out`); anything else must fail visibly — a
//! walker/compile error or a non-zero exit — never print something different.
//! The VM half of the harness (and the meaning of the lists) is in
//! `crush-lang-js/tests/node_corpus_test.rs`; this file mirrors
//! `python_parity.rs`.
//!
//! Per backend:
//! - `*_LOUD`: programs that must stay loud. One that starts matching fails
//!   the test ("now exact — remove it from the list").
//! - `*_SILENT`: programs that print wrong output today, keyed to the ticket
//!   that fixes them. Same bar as the VM: an unlisted silently-wrong program
//!   fails; a listed one that matches or turns loud must be moved/removed.
//!
//! Each backend runs both lowerings: swc (the `.js` default until CRUSH-250
//! step 6) and boa, whose lists are the swc lists adjusted by `BOA_EXACT` /
//! `BOA_LOUD`.
//!
//! Run with `--nocapture` for the per-area match / loud / silent table.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crush_aot::AotCompiler;
use crush_lang_js::JsLowering;

mod common;

/// Rust backend: programs that must stay loud — `(program, why / fixing ticket)`.
const RUST_LOUD: &[(&str, &str)] = &[
    ("arr_basic", "CRUSH-zorro-15: no indexOf/includes cap in AOT"),
    ("arr_hof", "CRUSH-zorro-20: callbacks; no join/reduce cap in AOT"),
    ("arr_push_loop_multi", "CRUSH-250: `push` with several arguments (fixed on boa)"),
    ("arr_push_multi", "CRUSH-250: `push` with several arguments (fixed on boa)"),
    ("arr_slice_splice", "CRUSH-zorro-15: no join cap in AOT"),
    ("arr_sort", "CRUSH-zorro-15/20: no sort/join cap in AOT"),
    ("arr_spread", "CRUSH-zorro-23: array spread (`[...a]`) is not supported by the JS walker yet"),
    ("arr_spread_call", "CRUSH-zorro-23: call-argument spread"),
    ("async_generator", "out of scope: generators"),
    ("async_then", "out of scope: async/Promise"),
    ("builtin_json", "CRUSH-zorro-17: JSON has no AOT runtime"),
    ("builtin_map_set", "CRUSH-zorro-17: Map/Set"),
    ("builtin_math", "CRUSH-zorro-17: math.* caps have no AOT lowering"),
    ("builtin_number_conv", "CRUSH-zorro-17: toString/toFixed"),
    ("class_accessor", "COSM objects aren't supported on the AOT backends yet (CRUSH-zorro-43)"),
    ("class_basic", "COSM objects aren't supported on the AOT backends yet (CRUSH-zorro-43)"),
    ("class_defaults", "COSM objects aren't supported on the AOT backends yet (CRUSH-zorro-43)"),
    ("class_destructuring", "COSM objects aren't supported on the AOT backends yet (CRUSH-zorro-43)"),
    ("class_extends", "COSM objects aren't supported on the AOT backends yet (CRUSH-zorro-43)"),
    ("class_fields", "COSM objects aren't supported on the AOT backends yet (CRUSH-zorro-43)"),
    ("class_static", "COSM objects aren't supported on the AOT backends yet (CRUSH-zorro-43)"),
    ("ctrl_finally", "CRUSH-zorro-7: try/finally has no AOT lowering (silently wrong on the VM)"),
    ("ctrl_for_in", "CRUSH-zorro-16: for-in; no join cap in AOT"),
    ("ctrl_for_in_array", "CRUSH-zorro-23: aot: the Rust AOT backend cannot compile: `type_of` (fn main, instruction 11); `throw` (fn main, instruction 63)"),
    ("ctrl_labels", "CRUSH-zorro-6: a labeled `continue` (`continue label`) is not supported by the JS walker yet"),
    ("ctrl_try_catch", "CRUSH-zorro-19: try/throw has no AOT lowering"),
    ("fn_arrow", "CRUSH-251: AOT backends can't compile `make_closure` (function values, #194) yet"),
    ("fn_arrow_two", "CRUSH-251: AOT backends can't compile `make_closure` (function values, #194) yet"),
    ("fn_closure_counter", "CRUSH-zorro-20: closure capture (CRUSH-251)"),
    ("fn_higher_order", "CRUSH-zorro-20: function values (CRUSH-251)"),
    ("fn_closure_let_loop", "closures are VM-only (CRUSH-251)"),
    ("fn_default_param", "CRUSH-zorro-18: a default parameter value (`x = …`) is not supported by the swc walker"),
    ("fn_default_param_more", "CRUSH-zorro-18: a default parameter value (`x = …`) is not supported by the swc walker"),
    ("fn_rest_param", "CRUSH-zorro-18: a rest parameter (`...args`) is not supported by the JS walker yet"),
    ("fn_rest_param_more", "CRUSH-zorro-18: a rest parameter (`...args`) is not supported by the JS walker yet"),
    // A top-level function can't see the script's variables on swc: it reads
    // them unassigned, which fails as on the VM (they read as null before).
    ("fn_script_late", "CRUSH-zorro-54: reads a script variable unassigned (the VM fails too); boa owns script scope (CRUSH-250)"),
    ("fn_script_vars", "CRUSH-zorro-54: reads a script variable unassigned (the VM fails too); boa owns script scope (CRUSH-250)"),
    ("misc_console_multi", "CRUSH-zorro-12: `undefined` is an unassigned local on swc, now loud (CRUSH-zorro-54)"),
    ("misc_console_error", "CRUSH-zorro-23: `console.error` / `console.warn` (stderr output) is not supported by the JS walker yet"),
    ("obj_computed_key", "CRUSH-zorro-23: a computed property key (`[k]: v`) is not supported by the JS walker yet"),
    ("obj_destructure", "CRUSH-zorro-16: destructuring"),
    ("obj_literal", "CRUSH-zorro-16: Object.keys; no join cap in AOT"),
    ("obj_setter", "CRUSH-zorro-23: an object setter (`set x(v)`) is not supported by the JS walker yet"),
    ("obj_spread", "CRUSH-zorro-16: object spread"),
    ("ops_bitwise", "CRUSH-zorro-12: bitwise operators"),
    ("ops_division_inf", "CRUSH-zorro-12: Infinity/NaN from division"),
    ("ops_int_overflow", "CRUSH-zorro-13: number model"),
    ("ops_nan", "CRUSH-zorro-12: `NaN` is an unassigned local on swc, now loud (CRUSH-zorro-54)"),
    ("ops_null_eq_undefined", "CRUSH-zorro-12: `undefined` is an unassigned local on swc (matched by accident while it read as null; loud since CRUSH-zorro-54)"),
    ("ops_nullish", "CRUSH-zorro-12: ??"),
    ("ops_typeof", "CRUSH-zorro-12: typeof"),
    ("scope_block_shadow", "CRUSH-zorro-5: a nested `let`/`const`/`class`/`for`/`catch` binding that shadows an outer `x` is not supported by the JS walker yet"),
    ("scope_catch_shadow", "CRUSH-zorro-5: a `catch` parameter that shadows an outer binding"),
    ("scope_hoist_block_loop", "CRUSH-250: a nested `function` that uses its enclosing function's locals"),
    ("scope_hoist_late_let", "CRUSH-250: a nested `function` that uses its enclosing function's locals"),
    ("scope_nested_fn_decl", "CRUSH-250: a nested `function` that uses its enclosing function's locals"),
    ("scope_shadow_closure", "CRUSH-zorro-5: a block `let` that shadows an outer binding"),
    ("scope_shadow_loop", "CRUSH-zorro-5: a loop/block `let` that shadows an outer binding"),
    ("str_methods", "CRUSH-zorro-14: string methods"),
    ("str_methods2", "CRUSH-zorro-14: string methods"),
    ("str_replace", "CRUSH-zorro-14: string methods"),
];

/// Rust backend: silently-wrong programs today — `(program, fixing ticket, symptom)`.
/// Loud on the VM but silently wrong on AOT, because AOT reads an
/// out-of-range index / missing field as `null` where the VM errors. (An
/// unassigned local fails on AOT as on the VM since CRUSH-zorro-54.)
const RUST_SILENT: &[(&str, &str, &str)] = &[
    // AOT only (loud on the VM):
    ("misc_undefined", "CRUSH-zorro-12", "undefined / out-of-range index / missing field print `null`"),
    ("ops_optional_chain", "CRUSH-zorro-12", "o?.x?.y prints `null`, not `undefined`"),
];

/// C backend: identical to the Rust backend today. Split when they diverge.
const C_LOUD: &[(&str, &str)] = RUST_LOUD;
const C_SILENT: &[(&str, &str, &str)] = RUST_SILENT;

/// The boa lowering (CRUSH-250) on AOT: the swc lists above, adjusted by
/// these deltas (both backends). Programs boa runs exactly that are loud or
/// silently wrong with swc:
const BOA_EXACT: &[&str] = &[
    "arr_push_loop_multi",
    "arr_push_multi",
    "arr_spread_call",
    "fn_default_param",
    "fn_rest_param",
    "fn_rest_param_more",
    "scope_block_shadow",
    "scope_shadow_loop",
];
/// Silently wrong (or right by accident) on AOT with swc, refused at walk
/// time with boa:
const BOA_LOUD: &[(&str, &str)] = &[
    ("ops_optional_chain", "CRUSH-zorro-12: `?.` (refused at walk time)"),
    ("misc_console_multi", "CRUSH-zorro-12: `undefined` as a value (refused at walk time)"),
    ("ops_nan", "CRUSH-zorro-12: `NaN` as a value (refused at walk time)"),
    // A top-level function using a script variable is a closure over it on
    // boa (VM exact); AOT can't compile closures yet. On swc these read the
    // variables unassigned and fail (CRUSH-zorro-54) or, in
    // ops_ternary_nested, match because the function never runs.
    ("fn_script_late", "closures (CRUSH-251) don't compile on AOT yet"),
    ("fn_script_vars", "closures (CRUSH-251) don't compile on AOT yet"),
    ("ops_ternary_nested", "closures (CRUSH-251) don't compile on AOT yet"),
    // Loud on AOT with swc too: `undefined` is an unassigned local there.
    ("ops_null_eq_undefined", "CRUSH-zorro-12: `undefined` as a value (refused at walk time)"),
];

/// A backend's swc lists, adjusted for the boa lowering.
fn boa_lists<'a>(
    loud: &[(&'a str, &'a str)],
    silent: &[(&'a str, &'a str, &'a str)],
) -> (Vec<(&'a str, &'a str)>, Vec<(&'a str, &'a str, &'a str)>) {
    let moved = |n: &str| BOA_LOUD.iter().any(|e| e.0 == n) || BOA_EXACT.contains(&n);
    let mut l: Vec<_> = loud.iter().copied().filter(|e| !moved(e.0)).collect();
    l.extend(BOA_LOUD.iter().copied());
    let s = silent.iter().copied().filter(|e| !moved(e.0)).collect();
    (l, s)
}

fn corpus() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crush-lang-js/tests/node_corpus");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("corpus dir")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "js"))
        .collect();
    files.sort();
    assert!(files.len() >= 90, "corpus shrank: {}", files.len());
    files
}

enum Outcome {
    Exact,
    Loud(String),
    Wrong(String),
}

/// Program stdout without the type-tagged result line the runner prints last.
fn run(so: &Path, expected: &str) -> Outcome {
    let out = Command::new(env!("CARGO_BIN_EXE_crush-aot-runner")).arg(so).output().expect("spawn runner");
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Outcome::Loud(format!("exit {:?}: {}", out.status.code(), err.trim().lines().next().unwrap_or("")));
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let body = text.trim_end_matches('\n');
    let printed = match body.rfind('\n') {
        Some(i) => format!("{}\n", &body[..i]),
        None => String::new(),
    };
    if printed == expected { Outcome::Exact } else { Outcome::Wrong(printed) }
}

fn first_line(e: impl std::fmt::Display) -> String {
    format!("{e:#}").lines().next().unwrap_or("").to_string()
}

fn compile_and_run(backend: &str, lowering: JsLowering, name: &str, source: &str, expected: &str) -> Outcome {
    let frontend = crush_lang_js::JsFrontend::with_lowering("js", lowering);
    let cast = match crush_walker_core::frontend_pipeline(&frontend, source).map(|(_, p)| p) {
        Ok(c) => c,
        Err(e) => return Outcome::Loud(format!("walker: {}", first_line(e))),
    };
    let casm = match crush_frontend::compiler::Compiler::new().compile(cast) {
        Ok(c) => c,
        Err(e) => return Outcome::Loud(format!("compile: {}", first_line(e))),
    };
    let aot = AotCompiler::new();
    let tag = match lowering {
        JsLowering::Swc => "",
        JsLowering::Boa => "b",
    };
    let built = if backend == "rust" {
        aot.compile_casm(&casm, &format!("jsp_{name}_r{tag}"))
    } else {
        aot.compile_c(&casm, &format!("jsp_{name}_c{tag}"), "gcc")
    };
    match built {
        Ok(so) => run(&so, expected),
        Err(e) => Outcome::Loud(format!("aot: {}", first_line(e))),
    }
}

fn check(backend: &str, lowering: JsLowering, known_loud: &[(&str, &str)], known_silent: &[(&str, &str, &str)]) {
    let files = corpus();
    let mut problems = Vec::new();
    for n in known_loud.iter().map(|e| e.0).chain(known_silent.iter().map(|e| e.0)) {
        if !files.iter().any(|p| p.file_stem().is_some_and(|s| s == n)) {
            problems.push(format!("{n}: listed for {backend}, but no such corpus program"));
        }
    }
    let mut by_area: BTreeMap<String, [usize; 3]> = BTreeMap::new();
    // The label only: `compile_and_run` needs the bare backend name (with the
    // label it built every "rust" run with the C backend).
    let (engine, backend) = (backend, &format!("{backend} ({lowering:?} lowering)"));
    println!("\nJS corpus on AOT {backend}");
    // Build and run every program on the worker pool; check the outcomes in order.
    let outcomes = common::par_map(&files, |path| {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let source = std::fs::read_to_string(path).unwrap();
        let expected = std::fs::read_to_string(path.with_extension("out")).unwrap();
        compile_and_run(engine, lowering, &name, &source, &expected)
    });
    for (path, outcome) in files.iter().zip(outcomes) {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let expected = std::fs::read_to_string(path.with_extension("out")).unwrap();
        let loud_entry = known_loud.iter().find(|e| e.0 == name);
        let silent_entry = known_silent.iter().find(|e| e.0 == name);
        let slot = match outcome {
            Outcome::Exact => 0,
            Outcome::Loud(_) => 1,
            Outcome::Wrong(_) => 2,
        };
        by_area.entry(name.split('_').next().unwrap_or(&name).to_string()).or_default()[slot] += 1;
        match (&outcome, loud_entry, silent_entry) {
            (Outcome::Exact, None, None) => {}
            (Outcome::Exact, Some(_), _) => {
                problems.push(format!("{name}: now exact on {backend} — remove it from the loud list"))
            }
            (Outcome::Exact, _, Some((_, t, _))) => {
                problems.push(format!("{name}: now exact on {backend} — remove it from the silent list ({t})"))
            }
            (Outcome::Loud(why), Some(_), _) => println!("  loud as expected  {name}: {why}"),
            (Outcome::Loud(why), None, Some((_, t, _))) => problems.push(format!(
                "{name}: was silently wrong on {backend} ({t}), now loud ({why}) — move it to the loud list"
            )),
            (Outcome::Loud(why), None, None) => problems.push(format!("{name}: {backend} failed: {why}")),
            (Outcome::Wrong(_), _, Some((_, t, what))) => println!("  KNOWN SILENT      {name} [{t}]: {what}"),
            (Outcome::Wrong(got), _, None) => problems.push(format!(
                "{name}: {backend} printed WRONG output (SILENTLY WRONG):\n    node:  {expected:?}\n    crush: {got:?}"
            )),
        }
    }
    println!("\nJS corpus on AOT {backend}");
    println!("  {:<10} {:>5} {:>5} {:>6}", "area", "match", "loud", "silent");
    let mut total = [0; 3];
    for (a, c) in &by_area {
        println!("  {a:<10} {:>5} {:>5} {:>6}", c[0], c[1], c[2]);
        for (t, n) in total.iter_mut().zip(c) {
            *t += n;
        }
    }
    println!("  {:<10} {:>5} {:>5} {:>6}", "TOTAL", total[0], total[1], total[2]);
    assert!(problems.is_empty(), "{} problem(s) on {backend}:\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn rust_backend_is_exact_or_loud() {
    check("rust", JsLowering::Swc, RUST_LOUD, RUST_SILENT);
}

#[test]
fn c_backend_is_exact_or_loud() {
    if Command::new("gcc").arg("--version").output().map(|o| !o.status.success()).unwrap_or(true) {
        eprintln!("gcc not found; skipping the C backend");
        return;
    }
    check("c", JsLowering::Swc, C_LOUD, C_SILENT);
}


#[test]
fn rust_backend_is_exact_or_loud_on_boa() {
    let (loud, silent) = boa_lists(RUST_LOUD, RUST_SILENT);
    check("rust", JsLowering::Boa, &loud, &silent);
}

#[test]
fn c_backend_is_exact_or_loud_on_boa() {
    if Command::new("gcc").arg("--version").output().map(|o| !o.status.success()).unwrap_or(true) {
        eprintln!("gcc not found; skipping the C backend");
        return;
    }
    let (loud, silent) = boa_lists(C_LOUD, C_SILENT);
    check("c", JsLowering::Boa, &loud, &silent);
}
