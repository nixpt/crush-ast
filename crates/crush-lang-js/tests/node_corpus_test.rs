//! JS/TS parity harness (CRUSH-zorro-4): crush must print what `node` prints.
//!
//! Each `tests/node_corpus/<area>_<name>.js` runs through the walker → CAST →
//! CASM → VM pipeline (with the stdlib host capabilities registered) and is
//! compared **byte-for-byte** with `<name>.out`, produced by `node`. The TS
//! twin does the same for `tests/ts_corpus/*.ts` through the `ts` dialect,
//! against `node <file>.ts` (node strips the types itself). The AOT backends
//! run the JS corpus in `crates/crush-aot/tests/js_parity.rs`.
//!
//! Every program lands in one of three classes:
//! - **MATCH**: ran and printed exactly what node printed;
//! - **LOUD**: the walker, compiler or VM returned an error;
//! - **SILENTLY WRONG**: ran without error and printed something else.
//!
//! The bar is "never silently wrong". The lists below are the progress meter:
//! - `KNOWN_LOUD` — programs crush can't run yet. Each must stay loud; once it
//!   matches, the test fails and asks for the entry to be removed.
//! - `KNOWN_SILENT` — silently-wrong programs that exist today, each keyed to
//!   the ticket that fixes it. A listed program that starts matching, or turns
//!   loud, fails the test so the entry is moved/removed. Any silently-wrong
//!   program *not* listed here fails outright. Don't grow this list for new
//!   work: a new silent bug gets a ticket and a loud error first.
//!
//! The file prefix up to the first `_` is the feature area; run with
//! `--nocapture` for the per-area match / loud / silent table.
//!
//! With `js-boa` (a default feature) the harness also runs the Boa lowering
//! (CRUSH-250) against the same lists, minus `BOA_FIXED` (programs only boa
//! runs exactly), and prints the step-6 switch-over gate: boa at least as good
//! as swc in every area, with nothing silently wrong. A swc-less build
//! (`--no-default-features --features js-boa`) runs boa alone.

#![cfg(any(feature = "swc-backend", feature = "js-boa"))]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Programs that must fail loudly (an error, never wrong output) on the VM.
/// `(program, why it's loud today / which ticket fixes it)`.
const KNOWN_LOUD: &[(&str, &str)] = &[
    ("arr_basic", "CRUSH-zorro-15: indexOf/includes"),
    ("arr_hof", "CRUSH-zorro-20: map/filter/reduce callbacks"),
    ("arr_push_loop_multi", "CRUSH-250: `push` with several arguments (fixed on boa)"),
    ("arr_push_multi", "CRUSH-250: `push` with several arguments (fixed on boa)"),
    ("arr_slice_splice", "CRUSH-zorro-15: slice/splice"),
    ("arr_sort", "CRUSH-zorro-15/20: sort, sort(cmp)"),
    ("arr_spread", "CRUSH-zorro-23: array spread loses `...` (loud only via the out-of-range read)"),
    ("arr_spread_call", "CRUSH-zorro-23: call-argument spread"),
    ("async_generator", "out of scope: generators"),
    ("async_then", "out of scope: async/Promise"),
    ("builtin_json", "CRUSH-zorro-17: JSON"),
    ("builtin_map_set", "CRUSH-zorro-17: Map/Set"),
    ("builtin_math", "CRUSH-zorro-17: variadic Math.max"),
    ("builtin_number_conv", "CRUSH-zorro-17: parseInt/Number/String/toFixed"),
    ("class_accessor", "out of scope: classes (CRUSH-251 object model)"),
    ("class_basic", "out of scope: classes (CRUSH-251 object model)"),
    ("class_extends", "out of scope: classes (CRUSH-251 object model)"),
    ("class_fields", "out of scope: classes (CRUSH-251 object model)"),
    ("class_static", "out of scope: classes (CRUSH-251 object model)"),
    ("ctrl_finally", "CRUSH-zorro-7: `try … finally` is not supported by the JS walker yet"),
    ("ctrl_for_in", "CRUSH-zorro-16: for-in (+ join, CRUSH-zorro-15)"),
    ("ctrl_for_in_array", "CRUSH-zorro-23: `for-in` over an array/string is not supported by the JS walker yet"),
    ("ctrl_labels", "CRUSH-zorro-6: a labeled `continue` (`continue label`) is not supported by the JS walker yet"),
    ("ctrl_try_catch", "CRUSH-zorro-19: new Error / e.message"),
    ("fn_default_param", "CRUSH-zorro-18: default parameters (fixed on boa)"),
    ("fn_default_param_more", "CRUSH-zorro-18: default parameters (fixed on boa)"),
    ("fn_script_late", "CRUSH-250: a top-level function reading a script variable (fixed on boa)"),
    ("fn_script_vars", "CRUSH-250: a top-level function reading a script variable (fixed on boa)"),
    ("fn_rest_param", "CRUSH-zorro-18: rest parameters (fixed on boa)"),
    ("fn_rest_param_more", "CRUSH-zorro-18: rest parameters (fixed on boa)"),
    ("misc_console_error", "CRUSH-zorro-23: `console.error` / `console.warn` (stderr output) is not supported by the JS walker yet"),
    ("misc_console_multi", "CRUSH-zorro-12: `undefined` as a value"),
    ("misc_undefined", "CRUSH-zorro-12: `undefined` as a value"),
    ("obj_computed_key", "CRUSH-zorro-23: a computed property key (`[k]: v`) is not supported by the JS walker yet"),
    ("obj_destructure", "CRUSH-zorro-16: destructuring"),
    ("obj_literal", "CRUSH-zorro-16: Object.keys"),
    ("obj_setter", "CRUSH-zorro-23: an object setter (`set x(v)`) is not supported by the JS walker yet"),
    ("obj_spread", "CRUSH-zorro-16: object spread"),
    ("ops_bitwise", "CRUSH-zorro-12: bitwise operators"),
    ("ops_division_inf", "CRUSH-zorro-12: Infinity/NaN from division"),
    ("ops_int_overflow", "CRUSH-zorro-13: number model"),
    ("ops_nan", "CRUSH-zorro-12: NaN"),
    ("ops_null_eq_undefined", "CRUSH-zorro-12: `undefined` as a value"),
    ("ops_nullish", "CRUSH-zorro-12: ??"),
    ("ops_optional_chain", "CRUSH-zorro-12: ?."),
    ("ops_typeof", "CRUSH-zorro-12: typeof"),
    ("scope_block_shadow", "CRUSH-zorro-5: a nested `let`/`const`/`class`/`for`/`catch` binding that shadows an outer `x` is not supported by the JS walker yet"),
    ("scope_catch_shadow", "CRUSH-zorro-5: a `catch` parameter that shadows an outer binding"),
    ("scope_hoist_block_loop", "CRUSH-250: a nested `function` that uses its enclosing function's locals (fixed on boa)"),
    ("scope_hoist_late_let", "CRUSH-250: a nested `function` that uses its enclosing function's locals (fixed on boa)"),
    ("scope_nested_fn_decl", "CRUSH-250: a nested `function` that uses its enclosing function's locals (fixed on boa)"),
    ("scope_shadow_closure", "CRUSH-zorro-5: a block `let` that shadows an outer binding"),
    ("scope_shadow_loop", "CRUSH-zorro-5: a loop/block `let` that shadows an outer binding"),
    ("str_methods", "CRUSH-zorro-14: string methods"),
    ("str_methods2", "CRUSH-zorro-14: string methods"),
    ("str_replace", "CRUSH-zorro-14: string methods"),
];

/// Programs the boa lowering (CRUSH-250) runs exactly that the swc lowering
/// can't: on boa they are expected to match, not to be loud.
const BOA_FIXED: &[(&str, &str)] = &[
    ("class_accessor", "get/set accessors on COSM (CRUSH-277)"),
    ("class_basic", "classes, `new` and `this` on COSM (CRUSH-277)"),
    ("class_extends", "`extends` + `super.m()` on COSM (CRUSH-277)"),
    ("class_fields", "class fields (D7 injection) on COSM (CRUSH-277)"),
    ("class_static", "static members on COSM (CRUSH-277)"),
    ("arr_push_loop_multi", "`push(a, b, …)` evaluates the arguments, then pushes each"),
    ("arr_push_multi", "`push(a, b, …)` evaluates the arguments, then pushes each"),
    ("fn_default_param", "a call passes defaulted arguments with a \"given\" flag (CRUSH-zorro-18)"),
    ("fn_default_param_more", "a call passes defaulted arguments with a \"given\" flag (CRUSH-zorro-18)"),
    ("fn_rest_param", "a call packs the rest arguments into an array (CRUSH-zorro-18)"),
    ("fn_rest_param_more", "a call packs the rest arguments into an array (CRUSH-zorro-18)"),
    ("fn_script_late", "a top-level function using script variables is a closure over them"),
    ("fn_script_vars", "a top-level function using script variables is a closure over them"),
    ("scope_block_shadow", "block bindings get their own crush local (alpha-renamed)"),
    ("scope_catch_shadow", "block bindings get their own crush local (alpha-renamed)"),
    ("scope_hoist_block_loop", "a scope's `let`/`const` exist before the closures hoisted into it"),
    ("scope_hoist_late_let", "a scope's `let`/`const` exist before the closures hoisted into it"),
    ("scope_nested_fn_decl", "a nested `function` is a hoisted closure"),
    ("scope_shadow_closure", "block bindings get their own crush local (alpha-renamed)"),
    ("scope_shadow_loop", "block bindings get their own crush local (alpha-renamed)"),
];

/// Programs that are silently wrong on the VM today: `(program, fixing ticket, symptom)`.
const KNOWN_SILENT: &[(&str, &str, &str)] = &[
];

/// TS twin: programs that must stay loud under the `ts` dialect.
#[cfg(feature = "ts")]
const TS_KNOWN_LOUD: &[(&str, &str)] = &[
    ("class_fields", "out of scope: classes (CRUSH-251 object model)"),
    ("ctrl_finally", "CRUSH-zorro-7: `try … finally` is not supported by the JS walker yet"),
    ("fn_default_param", "CRUSH-zorro-18: default parameters"),
    ("fn_optional_param", "CRUSH-zorro-18: missing argument is `undefined`"),
    ("str_methods", "CRUSH-zorro-14: string methods"),
];

/// TS twin: silently-wrong programs, keyed to their fixing ticket.
#[cfg(feature = "ts")]
const TS_KNOWN_SILENT: &[(&str, &str, &str)] = &[
];

fn corpus(dir: &str, ext: &str) -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(dir);
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("corpus dir")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == ext))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "empty corpus in {}", dir.display());
    files
}

fn stem(path: &Path) -> String {
    path.file_stem().unwrap().to_string_lossy().into_owned()
}

fn area(name: &str) -> &str {
    name.split('_').next().unwrap_or(name)
}

/// One way of turning source into CAST.
#[derive(Clone, Copy)]
enum Lowering {
    /// The swc lowering, called directly so it is what's measured whatever
    /// `js_to_cast` defaults to.
    #[cfg(feature = "swc-backend")]
    Swc,
    /// `js_to_cast(src, "ts")` — the TS dialect (swc).
    #[cfg(feature = "ts")]
    Ts,
    /// The Boa lowering, called directly (report only, behind `js-boa`).
    #[cfg(feature = "js-boa")]
    Boa,
}

fn lower(source: &str, how: Lowering) -> anyhow::Result<crush_cast::Program> {
    match how {
        #[cfg(feature = "swc-backend")]
        Lowering::Swc => {
            let module = crush_lang_js::backend::parse(source, "js")?;
            let ctx = crush_walker_core::LowerCtx::new(source, "input.js", "javascript");
            crush_lang_js::lower_swc::lower_module(&module, &ctx)
        }
        #[cfg(feature = "ts")]
        Lowering::Ts => crush_lang_js::js_to_cast(source, "ts"),
        #[cfg(feature = "js-boa")]
        Lowering::Boa => {
            let ast = crush_lang_js::backend::boa::parse(source)?;
            let ctx = crush_walker_core::LowerCtx::new(source, "input.js", "javascript");
            crush_lang_js::lower::lower_boa(ast, &ctx)
        }
    }
}

fn run_crush(source: &str, how: Lowering) -> anyhow::Result<String> {
    let cast = lower(source, how)?;
    let casm = crush_frontend::compiler::Compiler::new()
        .compile(cast)
        .map_err(|e| anyhow::anyhow!("CAST→CASM: {e}"))?;
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm)?;
    let quotas = crush_vm::Quotas { max_steps: 10_000_000, ..Default::default() };
    let caps = crush_lang_sdk::HostCapsBuilder::new().stdlib(true).build();
    Ok(crush_vm::run_with_caps(&vm, &quotas, Some(&caps))?.output)
}

enum Outcome {
    Match,
    Loud(String),
    Silent(String),
}

fn classify(source: &str, expected: &str, how: Lowering) -> Outcome {
    match std::panic::catch_unwind(|| run_crush(source, how)) {
        Ok(Ok(got)) if got == expected => Outcome::Match,
        Ok(Ok(got)) => Outcome::Silent(got),
        Ok(Err(e)) => Outcome::Loud(format!("{e:#}").lines().next().unwrap_or("").to_string()),
        Err(_) => Outcome::Loud("panicked".to_string()),
    }
}

/// Per-area `[match, loud, silent]` counts.
#[derive(Default)]
struct Tally {
    by_area: BTreeMap<String, [usize; 3]>,
}

impl Tally {
    fn add(&mut self, name: &str, o: &Outcome) {
        let i = match o {
            Outcome::Match => 0,
            Outcome::Loud(_) => 1,
            Outcome::Silent(_) => 2,
        };
        self.by_area.entry(area(name).to_string()).or_default()[i] += 1;
    }

    fn print(&self, title: &str) {
        println!("\n{title}");
        println!("  {:<10} {:>5} {:>5} {:>6}", "area", "match", "loud", "silent");
        let mut total = [0; 3];
        for (a, c) in &self.by_area {
            println!("  {a:<10} {:>5} {:>5} {:>6}", c[0], c[1], c[2]);
            for (t, n) in total.iter_mut().zip(c) {
                *t += n;
            }
        }
        println!("  {:<10} {:>5} {:>5} {:>6}", "TOTAL", total[0], total[1], total[2]);
    }
}

/// Run a corpus, enforce the lists, print the per-area table, return the problems.
fn check(
    title: &str,
    files: &[PathBuf],
    how: Lowering,
    known_loud: &[(&str, &str)],
    known_silent: &[(&str, &str, &str)],
) -> (Tally, Vec<String>) {
    let mut problems = Vec::new();
    let names: Vec<String> = files.iter().map(|p| stem(p)).collect();
    for n in known_loud.iter().map(|e| e.0).chain(known_silent.iter().map(|e| e.0)) {
        if !names.iter().any(|x| x == n) {
            problems.push(format!("{n}: listed in {title}, but no such corpus program"));
        }
    }
    println!("\n{title}");
    let mut tally = Tally::default();
    for path in files {
        let name = stem(path);
        let source = std::fs::read_to_string(path).unwrap();
        let expected = std::fs::read_to_string(path.with_extension("out")).unwrap();
        let loud_entry = known_loud.iter().find(|e| e.0 == name);
        let silent_entry = known_silent.iter().find(|e| e.0 == name);
        let outcome = classify(&source, &expected, how);
        tally.add(&name, &outcome);
        match (&outcome, loud_entry, silent_entry) {
            (Outcome::Match, None, None) => {}
            (Outcome::Match, Some(_), _) => {
                problems.push(format!("{name}: now exact — remove it from the known-loud list"))
            }
            (Outcome::Match, _, Some((_, t, _))) => {
                problems.push(format!("{name}: now exact — remove it from the known-silent list ({t})"))
            }
            (Outcome::Loud(why), Some(_), _) => println!("  loud as expected  {name}: {why}"),
            (Outcome::Loud(why), None, Some((_, t, _))) => problems.push(format!(
                "{name}: was silently wrong ({t}), now loud ({why}) — move it to the known-loud list"
            )),
            (Outcome::Loud(why), None, None) => problems.push(format!("{name}: LOUD (not allow-listed): {why}")),
            (Outcome::Silent(_), _, Some((_, t, what))) => println!("  KNOWN SILENT      {name} [{t}]: {what}"),
            (Outcome::Silent(got), _, None) => problems.push(format!(
                "{name}: SILENTLY WRONG\n    node:  {expected:?}\n    crush: {got:?}"
            )),
        }
    }
    tally.print(title);
    (tally, problems)
}

fn assert_clean(title: &str, problems: Vec<String>) {
    assert!(problems.is_empty(), "{} problem(s) in {title}:\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn crush_output_matches_node() {
    let files = corpus("tests/node_corpus", "js");
    assert!(files.len() >= 90, "corpus shrank: {}", files.len());
    let mut problems = Vec::new();
    #[cfg(feature = "swc-backend")]
    let swc = {
        let (tally, p) = check("JS corpus on the VM (swc lowering)", &files, Lowering::Swc, KNOWN_LOUD, KNOWN_SILENT);
        problems.extend(p);
        tally
    };
    #[cfg(feature = "js-boa")]
    {
        for (n, _) in BOA_FIXED {
            if !KNOWN_LOUD.iter().any(|e| e.0 == *n) {
                problems.push(format!("{n}: in BOA_FIXED but not in KNOWN_LOUD (fixed on swc too? drop it)"));
            }
        }
        let boa_loud: Vec<(&str, &str)> =
            KNOWN_LOUD.iter().copied().filter(|(n, _)| !BOA_FIXED.iter().any(|f| f.0 == *n)).collect();
        let title = "JS corpus on the VM (boa lowering)";
        let (boa, p) = check(title, &files, Lowering::Boa, &boa_loud, KNOWN_SILENT);
        problems.extend(p.into_iter().map(|p| format!("[boa] {p}")));
        #[cfg(feature = "swc-backend")]
        {
            // CRUSH-250 step 6 flips `.js` to boa once this holds.
            println!("\nswc vs boa per area [match, loud, silent]:");
            let mut gate = true;
            for (a, s) in &swc.by_area {
                let b = boa.by_area.get(a).copied().unwrap_or_default();
                let ok = b[0] >= s[0] && b[2] == 0;
                gate &= ok;
                println!("  {a:<10} swc {s:?}  boa {b:?}{}", if ok { "" } else { "  <- boa behind" });
            }
            println!("switch-over gate (boa >= swc in every area, 0 silent): {}", if gate { "PASS" } else { "not yet" });
        }
        #[cfg(not(feature = "swc-backend"))]
        let _ = boa;
    }
    #[cfg(feature = "swc-backend")]
    let _ = swc;
    assert_clean("the JS corpus", problems);
}

#[cfg(feature = "ts")]
#[test]
fn ts_twin_matches_node() {
    let files = corpus("tests/ts_corpus", "ts");
    let title = "TS twin on the VM (ts dialect)";
    let (_, problems) = check(title, &files, Lowering::Ts, TS_KNOWN_LOUD, TS_KNOWN_SILENT);
    assert_clean("the TS twin", problems);
}

fn node_available() -> bool {
    std::process::Command::new("node").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// Every `.out` must be what `node` prints today (stdout only). node 26 runs
/// `.ts` in strip-only mode, so the TS twin holds only erasable syntax: `enum`
/// and `namespace` have no node oracle (node 26 dropped
/// `--experimental-transform-types`) and are tested elsewhere (CRUSH-zorro-21).
#[test]
fn expected_outputs_match_node() {
    if !node_available() {
        eprintln!("node not on PATH — skipping .out freshness check");
        return;
    }
    let mut stale = Vec::new();
    let all = corpus("tests/node_corpus", "js").into_iter().chain(corpus("tests/ts_corpus", "ts"));
    for path in all {
        let out = std::process::Command::new("node").arg(&path).output().unwrap();
        let expected = std::fs::read_to_string(path.with_extension("out")).unwrap();
        if !out.status.success() || String::from_utf8_lossy(&out.stdout) != expected {
            stale.push(format!("{}: .out is stale (or node failed)", path.display()));
        }
    }
    assert!(stale.is_empty(), "{}", stale.join("\n"));
}

// swc only: the boa lowering accepts the comma expression (CRUSH-250).
#[cfg(feature = "swc-backend")]
#[test]
fn side_effecting_comma_in_value_position_fails_loudly() {
    let err = crush_lang_js::js_to_cast("var i = 0; var x = (i++, 5); console.log(x);", "js").unwrap_err();
    assert!(format!("{err:#}").contains("comma expression"), "{err:#}");
}
