//! Inline lowering of `@python` / `@javascript` blocks (CRUSH-272,
//! `PolyglotMode::Lower`): the block's body is lowered through its walker
//! into the program instead of running in `python3` / `node`.
//!
//! The bar is exact or loud. Every program of the walker's CPython corpus,
//! used as an `@python` block body, prints what CPython prints — on the
//! scheduler VM (native) and on the portable VM (the browser's) — without a
//! `polyglot.python` grant, since nothing is spawned. A construct the walker
//! can't lower fails compilation with the construct and the block's line.
#![cfg(all(feature = "polyglot-python", feature = "polyglot-javascript"))]

use std::path::{Path, PathBuf};

use crush_lang_sdk::compile::{CompileOptions, PolyglotMode, compile_crush_source_with};

const LOWER: CompileOptions = CompileOptions { polyglot: PolyglotMode::Lower };

fn compile(source: &str) -> anyhow::Result<crush_vm::Program> {
    compile_crush_source_with(source, &LOWER)
}

/// Run on both VMs, with no host capabilities at all (so no polyglot grant),
/// and require them to agree.
fn run(source: &str) -> Result<String, String> {
    let program = compile(source).map_err(|e| format!("compile: {e:#}"))?;
    let native = crush_vm::run_with_caps(&program, &crush_vm::Quotas::default(), None)
        .map(|r| r.output)
        .map_err(|e| format!("runtime: {e}"));
    let portable = crush_vm::portable_vm::PortableVm::new(program)
        .run()
        .map(|r| r.output)
        .map_err(|e| format!("runtime: {e}"));
    assert_eq!(native, portable, "scheduler and portable VMs disagree on:\n{source}");
    native
}

/// `body` as a `@lang<arrow>` block in `main`, indented as an author would,
/// with Crush statements `before` and `after` it.
fn block(lang: &str, arrow: &str, before: &str, body: &str, after: &str) -> String {
    let indented: String = body.lines().map(|l| format!("        {l}\n")).collect();
    format!("fn main() {{\n{before}    @{lang}{arrow} {{\n{indented}    }}\n{after}}}\n")
}

fn corpus() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crush-lang-python/tests/cpython_corpus");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("corpus dir")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "py"))
        .collect();
    files.sort();
    assert!(files.len() >= 23, "corpus shrank: {}", files.len());
    files
}

#[test]
fn cpython_corpus_as_python_blocks_matches_cpython() {
    let mut failures = Vec::new();
    for path in corpus() {
        let body = std::fs::read_to_string(&path).unwrap();
        let expected = std::fs::read_to_string(path.with_extension("out")).unwrap();
        // An explicit output, so the implicit rule doesn't pick a function
        // (21_lambda's `double`) to marshal back — see
        // `a_function_output_is_loud`.
        let source = block("python", " -> done", "", &format!("{body}done = True\n"), "");
        match run(&source) {
            Ok(got) if got == expected => {}
            Ok(got) => failures.push(format!("{}:\n  cpython: {expected:?}\n  lowered: {got:?}", path.display())),
            Err(e) => failures.push(format!("{}: {e}", path.display())),
        }
    }
    assert!(failures.is_empty(), "{} block(s) diverge from CPython:\n{}", failures.len(), failures.join("\n"));
}

fn compile_err(source: &str) -> String {
    match compile(source) {
        Ok(_) => panic!("expected a compile error for:\n{source}"),
        Err(e) => format!("{e:#}"),
    }
}

#[test]
fn inputs_and_outputs_marshal_through_lowering() {
    let src = block(
        "python",
        " -> (total, label, doubled)",
        "    let xs = [1, 2, 3]\n    let name = \"crush\"\n    let scale = 2.5\n    let cfg = {a: 10, b: 20}\n",
        "import math\ntotal = xs[0] + xs[1] + xs[2] + cfg[\"a\"]\nlabel = name.upper() + \"!\"\ndoubled = [x * scale for x in xs]\nprint(math.sqrt(16))",
        "    print(total)\n    print(label)\n    print(doubled)\n",
    );
    assert_eq!(run(&src).unwrap(), "4.0\n16\nCRUSH!\n[2.5, 5.0, 7.5]\n");
}

#[test]
fn a_block_cannot_change_a_crush_variable_it_does_not_declare() {
    // `xs.append` changes the block's copy, not the Crush array; the block's
    // own `x` is its local, not the Crush `x` — as in a subprocess.
    let src = block(
        "python",
        " -> (n, y)",
        "    let xs = [1, 2]\n    let x = 1\n",
        "xs.append(3)\nn = len(xs)\nx = 99\ny = x + 1",
        "    print(len(xs))\n    print(n)\n    print(x)\n    print(y)\n",
    );
    assert_eq!(run(&src).unwrap(), "2\n3\n1\n100\n");
}

#[test]
fn a_declared_output_overwrites_its_crush_variable() {
    let src = block("python", " -> r", "    let r = 0\n    let k = 3\n", "def add(v):\n    return v + k\nr = add(4)", "    print(r)\n");
    assert_eq!(run(&src).unwrap(), "7\n");
}

#[test]
fn an_output_returned_twice_is_two_copies() {
    let src = block("python", " -> (a, b)", "", "a = [1]\nb = a", "    a[0] = 5\n    print(a)\n    print(b)\n");
    assert_eq!(run(&src).unwrap(), "[5]\n[1]\n");
}

#[test]
fn an_output_bound_only_conditionally_fails_at_run_time() {
    let src = block("python", " -> r", "    let flag = false\n", "if flag:\n    r = 1", "    print(r)\n");
    assert!(run(&src).is_err(), "an unbound output must not read as null");
}

#[test]
fn a_function_output_is_loud() {
    // The implicit-output rule picks `f`: JSON can't carry a function, so the
    // subprocess fails on it, and so does the lowered block.
    let src = block("python", "", "", "f = lambda v: v + 1\nprint(f(1))", "");
    let e = run(&src).unwrap_err();
    assert!(e.contains("`f`") && e.contains("function"), "{e}");
}

#[test]
fn a_construct_the_walker_cannot_lower_is_a_compile_error_naming_it_and_the_line() {
    for (body, construct) in [
        ("class Point:\n    pass", "class definitions"),
        ("for a, b in [[1, 2]]:\n    pass", "for loop target must be a name"),
        ("print(math.pi)", "module attribute `math.pi`"),
        ("print(math.trunc(2.5))", "`trunc` is not supported by the walker yet"),
        ("with open('f') as fh:\n    pass", "with statements"),
        ("return 1", "`return` outside a function"),
        ("import json as j", "`import json as j`"),
        ("from math import floor", "`import from math import"),
        ("x = math.floor(2.5)", "`math.floor()` is not supported yet"),
        ("print((1, 2))", "tuples are not supported yet"),
        ("print(sum([1, 2]))", "`sum` is not supported by the walker yet"),
        ("print(str(1))", "`str` is not supported by the walker yet"),
        ("def f():\n    return 1, 2", "tuples are not supported yet"),
    ] {
        let e = compile_err(&block("python", "", "    let z = 1\n", body, ""));
        assert!(e.contains(construct), "{body:?}: {e}");
        assert!(e.contains("line 3") && e.contains("@python"), "{body:?}: {e}");
        assert!(e.contains("not as CPython") && e.contains("--polyglot"), "{body:?}: {e}");
    }
}

#[test]
fn a_block_cannot_call_a_crush_function() {
    let src = "fn helper(x) {\n    return x + 1\n}\nfn main() {\n    @python -> r {\n        r = helper(1)\n    }\n    print(r)\n}\n";
    let e = compile_err(src);
    assert!(e.contains("`helper`") && e.contains("Crush function"), "{e}");
}

#[test]
fn a_block_inside_try_is_refused_not_spawned() {
    let src = "fn main() {\n    try {\n        @python {\n            print(1)\n        }\n    } catch e {\n        print(e)\n    }\n}\n";
    let e = compile_err(src);
    assert!(e.contains("line 3") && e.contains("doesn't reach"), "{e}");
}

#[test]
fn a_lowered_program_needs_no_polyglot_grant() {
    let src = block("python", " -> r", "    let n = 4\n", "r = n * n", "    print(r)\n");
    let program = compile(&src).unwrap();
    let used = crush_lang_sdk::effects::used_by(&program).unwrap();
    assert!(!used.iter().any(|c| c.starts_with("polyglot.")), "{used:?}");
    assert!(crush_lang_sdk::effects::missing_grants(&program, Some(&crush_vm::HostCaps::new())).unwrap().is_empty());
    // The default stays a subprocess behind the grant.
    let exec = crush_lang_sdk::compile::compile_crush_source(&src).unwrap();
    assert!(crush_lang_sdk::effects::used_by(&exec).unwrap().contains("polyglot.python"));
}

#[test]
fn other_languages_are_left_to_exec_lang() {
    let src = "fn main() {\n    @bash {\n        echo hi\n    }\n}\n";
    let program = compile(src).unwrap();
    assert!(crush_lang_sdk::effects::used_by(&program).unwrap().contains("polyglot.bash"));
}

#[test]
fn javascript_blocks_lower_with_closures_and_outputs() {
    let src = block(
        "javascript",
        " -> out",
        "    let n = 4\n",
        "const xs = [];\nfor (let i = 0; i < n; i++) { xs.push(i * i); }\nfunction sum(a) { let t = 0; for (const v of a) { t += v; } return t + n; }\nlet out = sum(xs);\nconsole.log(\"js\", out, xs);",
        "    print(out)\n",
    );
    assert_eq!(run(&src).unwrap(), "js 18 [ 0, 1, 4, 9 ]\n18\n");
}

#[test]
fn a_javascript_construct_the_walker_cannot_lower_names_the_crush_line() {
    let src = block("javascript", "", "    let z = 1\n", "let a = 1;\nclass A {}", "");
    let e = compile_err(&src);
    assert!(e.contains("@javascript") && e.contains("line 3") && e.contains("a class"), "{e}");
    assert!(e.contains("not as Node"), "{e}");
    // Where the walker knows the construct's position, that `.crush` line too.
    let src = block("javascript", "", "    let z = 1\n", "let a = 1;\nlet b = a?.c;", "");
    let e = compile_err(&src);
    assert!(e.contains("line 3:") && e.contains("at line 5"), "{e}");
}

#[test]
fn javascript_top_level_return_is_a_syntax_error() {
    let e = compile_err(&block("javascript", "", "", "return 1;", ""));
    assert!(e.contains("syntax error"), "{e}");
}

/// The JS walker's node corpus, each program as a `@javascript` block body,
/// run with the stdlib (ambient, as in the corpus's own harness). Every
/// program must print exactly what node printed or fail loudly — never print
/// something else. The JS walker's own harness keeps the list of which are
/// loud; here the count of exact ones guards against the lowering losing any.
#[test]
fn node_corpus_as_javascript_blocks_is_exact_or_loud() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crush-lang-js/tests/node_corpus");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("corpus dir")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "js"))
        .collect();
    files.sort();
    let caps = crush_lang_sdk::HostCapsBuilder::new().stdlib(true).build();
    let quotas = crush_vm::Quotas { max_steps: 10_000_000, ..Default::default() };
    let (mut exact, mut loud, mut silent) = (Vec::new(), Vec::new(), Vec::new());
    for path in &files {
        let body = std::fs::read_to_string(path).unwrap();
        let expected = std::fs::read_to_string(path.with_extension("out")).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        // An explicit output, as for the CPython corpus.
        let source = block("javascript", " -> done", "", &format!("{body}\nlet done = 1;\n"), "");
        let got = compile(&source).and_then(|p| {
            Ok(crush_vm::run_with_caps(&p, &quotas, Some(&caps))?.output)
        });
        match got {
            Ok(out) if out == expected => exact.push(name),
            Ok(out) => silent.push(format!("{name}:\n  node:    {expected:?}\n  lowered: {out:?}")),
            Err(e) => loud.push(format!("{name}: {}", format!("{e:#}").lines().next().unwrap_or(""))),
        }
    }
    eprintln!("node corpus as @javascript blocks: {} exact, {} loud", exact.len(), loud.len());
    for l in &loud {
        eprintln!("  loud {l}");
    }
    assert!(silent.is_empty(), "silently wrong:\n{}", silent.join("\n"));
    assert!(exact.len() >= MIN_EXACT_JS, "only {} exact (expected >= {MIN_EXACT_JS}): {exact:?}", exact.len());
}

/// Exact node-corpus programs as `@javascript` blocks when this was written:
/// the 81 the boa lowering runs exactly, less 5 whose source the Crush lexer
/// can't capture as a block body (a `'` in a comment, an empty `''`).
const MIN_EXACT_JS: usize = 76;
