//! CRUSH-zorro-23: every construct the swc lowering used to drop or mis-lower
//! silently now fails at walk time, naming the construct; the cases next to
//! them that already worked still run and print what `node` prints.
//!
//! Each `loud` case is a program that used to run and print something other
//! than node's output (or nothing). When an area ticket implements one of
//! them, move it to `still_works` with node's output.

// The swc lowering only; the Boa lowering is a separate frontend (CRUSH-250).
#![cfg(feature = "ts")]

/// (ext, source, expected error substring)
const LOUD: &[(&str, &str, &str)] = &[
    // A default parameter was dropped: a call without the argument underflowed
    // the VM's stack and read `null` on AOT (CRUSH-zorro-18; boa supports it).
    (
        "js",
        "function g(a, b = 10) { return a + b; } console.log(g(1));",
        "a default parameter value (`x = …`) is not supported by the JS walker yet",
    ),
    (
        "js",
        "const f = (a = 1) => a; console.log(f());",
        "a default parameter value (`x = …`) is not supported by the JS walker yet",
    ),
    // Labeled jumps lost their label and acted on the innermost loop.
    (
        "js",
        "outer: for (let i = 0; i < 3; i++) { for (let j = 0; j < 3; j++) { if (i == 2) break outer; console.log(i, j); } }",
        "a labeled `break` (`break label`) is not supported by the JS walker yet",
    ),
    (
        "js",
        "outer: for (let i = 0; i < 3; i++) { for (let j = 0; j < 3; j++) { if (j == 1) continue outer; console.log(i, j); } }",
        "a labeled `continue` (`continue label`) is not supported by the JS walker yet",
    ),
    // `finally` never ran.
    ("js", "function f() { try { return 1; } finally { console.log('fin'); } } console.log(f());", "`try … finally` is not supported by the JS walker yet"),
    ("js", "try { console.log('a'); } catch (e) { console.log('c'); } finally { console.log('fin'); }", "`try … finally`"),
    ("js", "try { console.log('a'); } finally { console.log('f'); }", "`try … finally`"),
    // Shadowing: one slot per name, so the inner binding overwrote the outer one.
    ("js", "let x = 1; if (true) { let x = 2; } console.log(x);", "a nested `let`/`const`/`class`/`for`/`catch` binding that shadows an outer `x` is not supported by the JS walker yet"),
    ("js", "let x = 1; if (true) { { let x = 2; } } console.log(x);", "shadows an outer `x`"),
    ("js", "let x = 1; { let x = 2; } console.log(x);", "shadows an outer `x`"),
    ("js", "function f() { let x = 1; { const x = 2; } return x; } console.log(f());", "shadows an outer `x`"),
    ("js", "let x = 9; for (const x of [1, 2]) { } console.log(x);", "shadows an outer `x`"),
    ("js", "let e = 1; try { throw 5; } catch (e) { } console.log(e);", "shadows an outer `e`"),
    ("js", "let x = 1; switch (1) { case 1: { let x = 2; } } console.log(x);", "shadows an outer `x`"),
    ("js", "function f(x) { if (true) { let x = 2; } return x; } console.log(f(1));", "shadows an outer `x`"),
    ("js", "const g = (x) => { while (true) { const x = 2; break; } return x; }; console.log(g(1));", "shadows an outer `x`"),
    // Classes: members dropped or stored as the wrong thing; `extends` ignored.
    ("js", "class A { constructor() { console.log('ctor'); } } console.log('x');", "a class `constructor` is not supported by the JS walker yet"),
    ("js", "class A { static { console.log('static'); } } console.log('end');", "a class `static { … }` block is not supported by the JS walker yet"),
    ("js", "class A { #p() { return 1; } } console.log('x');", "a private class method (`#m()`) is not supported by the JS walker yet"),
    ("js", "class A { #n = 1; } console.log('x');", "a private class field (`#x`) is not supported by the JS walker yet"),
    ("js", "class A { get z() { return 2; } } console.log('x');", "a class getter or setter (`get x()` / `set x(v)`) is not supported by the JS walker yet"),
    ("js", "class A { x = 1; } console.log(A.x);", "a class instance field (`x = …` in a class body) is not supported by the JS walker yet"),
    ("js", "class B { } class A extends B { } console.log('x');", "class inheritance (`extends`) is not supported by the JS walker yet"),
    ("js", "const A = class { constructor() { console.log('c'); } }; console.log('x');", "a class `constructor`"),
    // Generators: `yield x` evaluated to `x`.
    ("js", "function* g() { yield 1; } console.log('x');", "a generator function (`function*`) is not supported by the JS walker yet"),
    ("js", "const o = { *g() { yield 1; } }; console.log('x');", "a generator function (`function*`)"),
    // Objects: spread dropped, computed keys all "[]", accessors wrong.
    ("js", "const a = {x: 1}; const b = {...a, y: 2}; console.log(b.x);", "object spread (`{...o}`) is not supported by the JS walker yet"),
    ("js", "const k = 'a'; const o = {[k]: 1}; console.log(o.a);", "a computed property key (`[k]: v`) is not supported by the JS walker yet"),
    ("js", "const o = { get z() { return 2; } }; console.log(o.z);", "an object getter (`get x()`) is not supported by the JS walker yet"),
    ("js", "const o = { set v(x) { console.log('set', x); } }; o.v = 1;", "an object setter (`set x(v)`) is not supported by the JS walker yet"),
    // Spread lost its `...`.
    ("js", "const a = [1, 2]; const b = [...a, 3]; console.log(b.length);", "array spread (`[...a]`) is not supported by the JS walker yet"),
    ("js", "function f(a, b) { return a + b; } const xs = [1, 2]; console.log(f(...xs));", "a spread argument (`f(...args)`) is not supported by the JS walker yet"),
    ("js", "function f(...xs) { console.log(xs); } f(1);", "a rest parameter (`...args`) is not supported by the JS walker yet"),
    ("js", "function f({a}) { console.log(a); } f({a: 1});", "a destructuring pattern is not supported by the JS walker yet"),
    // TS runtime declarations were dropped.
    ("ts", "enum C { R, G } console.log(C.G);", "a TypeScript `enum` is not supported by the JS walker yet"),
    ("ts", "export enum E { A } console.log('x');", "a TypeScript `enum`"),
    ("ts", "namespace N { export const x = 1; } console.log(N.x);", "a TypeScript `namespace` is not supported by the JS walker yet"),
    ("ts", "import fs = require('fs'); console.log('x');", "TypeScript `import x = …` is not supported by the JS walker yet"),
    // Export forms that became a comment-only block or dropped specifiers.
    ("js", "export * as ns from './m.js'; console.log('x');", "a namespace re-export (`export * as ns from …`) is not supported by the JS walker yet"),
    // console.error / console.warn went to stdout.
    ("js", "console.error('e'); console.log('o');", "`console.error` / `console.warn` (stderr output) is not supported by the JS walker yet"),
    ("js", "console.warn('w'); console.log('o');", "`console.error` / `console.warn` (stderr output)"),
    // Catch-alls that produced placeholders (`_expr`, `__crush_call__`, `new Object`, …).
    ("js", "const o = {n: 1}; o.n++; console.log(o.n);", "`++`/`--` on a property or element is not supported by the JS walker yet"),
    ("js", "function t(s, v) { return s[0] + v; } console.log(t`a${1}`);", "a tagged template literal (tag`…`) is not supported by the JS walker yet"),
    ("js", "(function () { console.log('iife'); })();", "calling an expression that isn't a plain name"),
    ("js", "const o = {a: {f: function () { return 3; }}}; console.log(o.a.f());", "calling a method on anything but a plain name"),
    ("js", "const o = {f: function () { return 3; }}; const k = 'f'; console.log(o[k]());", "a computed method call (`o[k]()`) is not supported by the JS walker yet"),
    ("js", "const m = {}; new m.X();", "`new` on anything but a plain name (`new a.B()`) is not supported by the JS walker yet"),
    ("js", "function F() { console.log(new.target === undefined); } F();", "`import.meta` / `new.target` is not supported by the JS walker yet"),
    ("js", "const r = /a/; console.log('x');", "a regular expression literal is not supported by the JS walker yet"),
    ("js", "const b = 1n; console.log('x');", "a BigInt literal (`1n`) is not supported by the JS walker yet"),
    ("js", "const o = { m() { return super.x; } }; console.log('x');", "`super.x` is not supported by the JS walker yet"),
    ("js", "import('./m.js'); console.log('x');", "dynamic `import(…)` is not supported by the JS walker yet"),
    ("jsx", "const e = <div />; console.log('x');", "JSX is not supported by the JS walker yet"),
];

/// (ext, source, node's stdout) — neighbours of the loud cases that already
/// worked and must keep working.
const STILL_WORKS: &[(&str, &str, &str)] = &[
    // Bare blocks are flattened everywhere. At the top level, in a `function`
    // declaration's body, and as a labeled block they used to be dropped.
    ("js", "{ console.log(5); } console.log(6);", "5\n6\n"),
    ("js", "function f() { { console.log(1); } console.log(2); } f();", "1\n2\n"),
    ("js", "l: { console.log(1); }", "1\n"),
    ("js", "let a = 1; { let b = 2; console.log(a + b); } { let b = 3; console.log(b); }", "3\n3\n"),
    ("js", "{ var v = 7; } console.log(v);", "7\n"),
    // Nested blocks inside if / loop / arrow bodies are flattened.
    ("js", "if (true) { { console.log(1); } } console.log(2);", "1\n2\n"),
    ("js", "for (let i = 0; i < 2; i++) { { console.log(i); } }", "0\n1\n"),
    ("js", "const f = () => { { console.log('a'); } return 1; }; console.log(f());", "a\n1\n"),
    // Fresh names in nested scopes, and sibling scopes reusing a name.
    ("js", "let a = 1; if (true) { let b = 2; console.log(a * b); }", "2\n"),
    ("js", "for (let i = 0; i < 2; i++) { } for (let i = 0; i < 2; i++) { console.log(i); }", "0\n1\n"),
    ("js", "if (false) { let t = 1; console.log(t); } else { let t = 2; console.log(t); }", "2\n"),
    ("js", "var v = 1; if (true) { var v = 2; } console.log(v);", "2\n"),
    // Unlabeled loops with break/continue, and a label nothing jumps to.
    ("js", "for (let i = 0; i < 5; i++) { if (i == 1) continue; if (i == 3) break; console.log(i); }", "0\n2\n"),
    ("js", "let n = 0; while (true) { n++; if (n > 2) break; } console.log(n);", "3\n"),
    ("js", "l: for (let i = 0; i < 2; i++) { console.log(i); }", "0\n1\n"),
    // try/catch without finally.
    ("js", "try { throw 'boom'; } catch (e) { console.log(e); }", "boom\n"),
    // Plain objects and arrays.
    ("js", "const o = {a: 1, b: 'x'}; console.log(o.a); console.log(o.b);", "1\nx\n"),
    ("js", "const xs = [1, 2, 3]; console.log(xs[1]); console.log(xs.length);", "2\n3\n"),
    ("js", "for (const k in {a: 1, b: 2}) { console.log(k); }", "a\nb\n"),
    // for-in over an object held in a variable passes the runtime type guard.
    ("js", "const o = {a: 1, b: 2}; for (const k in o) { console.log(k); }", "a\nb\n"),
    ("js", "function keys(o) { for (const k in o) { console.log(k); } } keys({x: 1});", "x\n"),
    ("js", "const o = { m() { return 4; } }; console.log('ok');", "ok\n"),
    // Static class members, plain `function` declarations and arrows.
    ("js", "class U { static n = 3; } console.log(U.n);", "3\n"),
    ("js", "function add(a, b) { return a + b; } console.log(add(2, 3));", "5\n"),
    // TS type-only declarations have no runtime effect.
    ("ts", "interface I { a: number } type T = number; const v: I = {a: 1}; console.log(v.a);", "1\n"),
    ("ts", "export interface I { a: number } console.log('x');", "x\n"),
    ("ts", "export default interface I { a: number } console.log('x');", "x\n"),
    ("ts", "declare enum D { A } declare namespace M { const y: number; } console.log('x');", "x\n"),
];

/// (source, expected runtime error substring): walks and compiles, then fails
/// when run. `for-in` over an array or string yielded its elements, not its
/// indices; whether a variable holds one is only known at run time.
const LOUD_AT_RUNTIME: &[(&str, &str)] = &[
    ("const a = [10, 20]; for (const k in a) { console.log(k); }", "`for-in` over an array/string is not supported by the JS walker yet"),
    ("const s = 'ab'; for (const k in s) { console.log(k); }", "`for-in` over an array/string is not supported by the JS walker yet"),
    ("for (const k in [10, 20]) { console.log(k); }", "`for-in` over an array/string"),
];

#[test]
fn for_in_over_a_sequence_fails_at_run_time() {
    let mut failures = Vec::new();
    for (src, want) in LOUD_AT_RUNTIME {
        match run_crush(src, "js") {
            Ok(out) => failures.push(format!("{src}\n  ran and printed {out:?}; expected: {want}")),
            Err(e) if format!("{e:#}").contains(want) => {}
            Err(e) => failures.push(format!("{src}\n  expected: {want}\n  got:      {e:#}")),
        }
    }
    assert!(failures.is_empty(), "{} case(s):\n{}", failures.len(), failures.join("\n"));
}

fn run_crush(source: &str, ext: &str) -> anyhow::Result<String> {
    let cast = crush_lang_js::js_to_cast(source, ext)?;
    let casm = crush_frontend::compiler::Compiler::new()
        .compile(cast)
        .map_err(|e| anyhow::anyhow!("CAST→CASM: {e}"))?;
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm)?;
    let quotas = crush_vm::Quotas { max_steps: 10_000_000, ..Default::default() };
    Ok(crush_vm::run_with_caps(&vm, &quotas, None)?.output)
}

#[test]
fn silently_mislowered_constructs_fail_at_walk_time() {
    let mut failures = Vec::new();
    for (ext, src, want) in LOUD {
        match crush_lang_js::js_to_cast(src, ext) {
            Ok(_) => failures.push(format!("[{ext}] {src}\n  walked without error; expected: {want}")),
            Err(e) if format!("{e:#}").contains(want) => {}
            Err(e) => failures.push(format!("[{ext}] {src}\n  expected: {want}\n  got:      {e:#}")),
        }
    }
    assert!(failures.is_empty(), "{} case(s):\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn neighbouring_constructs_still_match_node() {
    let mut failures = Vec::new();
    for (ext, src, want) in STILL_WORKS {
        match run_crush(src, ext) {
            Ok(got) if got == *want => {}
            Ok(got) => failures.push(format!("[{ext}] {src}\n  node:  {want:?}\n  crush: {got:?}")),
            Err(e) => failures.push(format!("[{ext}] {src}\n  {e:#}")),
        }
    }
    assert!(failures.is_empty(), "{} case(s):\n{}", failures.len(), failures.join("\n"));
}

/// The expected outputs above are node's: re-derive them when node is on PATH.
#[test]
fn still_works_expectations_match_node() {
    if std::process::Command::new("node").arg("--version").output().map(|o| !o.status.success()).unwrap_or(true) {
        eprintln!("node not on PATH — skipping expectation freshness check");
        return;
    }
    let dir = std::env::temp_dir().join(format!("crush-loud-constructs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (i, (ext, src, want)) in STILL_WORKS.iter().enumerate() {
        let path = dir.join(format!("case{i}.{ext}"));
        std::fs::write(&path, src).unwrap();
        let out = std::process::Command::new("node").arg(&path).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), *want, "[{ext}] {src}: expectation is stale");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
