//! Source-level imports (CRUSH-110), run through the real binaries: every
//! case writes two or more `.crush` files to a temp dir and runs `crush-run`
//! (or `crushc` / `crush-repl`) on them, with stdin closed.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn bin(name: &str) -> PathBuf {
    match name {
        "crush-run" => PathBuf::from(env!("CARGO_BIN_EXE_crush-run")),
        "crushc" => PathBuf::from(env!("CARGO_BIN_EXE_crushc")),
        "crush-repl" => PathBuf::from(env!("CARGO_BIN_EXE_crush-repl")),
        other => panic!("no binary {other}"),
    }
}

/// Write `files` (relative path → source) under a fresh temp dir.
fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (rel, src) in files {
        let path = dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, src).unwrap();
    }
    dir
}

/// `crush-run run <file> <args…>` in `dir`, stdin closed, no `CRUSH_PATH`.
fn run_in(dir: &Path, args: &[&str]) -> Output {
    run_env(dir, args, None)
}

fn run_env(dir: &Path, args: &[&str], crush_path: Option<&str>) -> Output {
    let mut cmd = Command::new(bin("crush-run"));
    cmd.arg("run").args(args).current_dir(dir).stdin(Stdio::null()).env_remove("CRUSH_PATH");
    if let Some(p) = crush_path {
        cmd.env("CRUSH_PATH", p);
    }
    cmd.output().unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// The program printed `expected` (its lines, ignoring the step summary) and
/// exited 0.
fn assert_prints(o: &Output, expected: &[&str]) {
    assert!(o.status.success(), "exit {:?}\nstdout: {}\nstderr: {}", o.status, stdout(o), stderr(o));
    let lines: Vec<String> =
        stdout(o).lines().filter(|l| !l.starts_with("[steps=")).map(str::to_string).collect();
    assert_eq!(lines, expected, "stderr: {}", stderr(o));
}

/// The compile failed (exit 1) and stderr contains every needle.
fn assert_fails(o: &Output, needles: &[&str]) {
    assert_eq!(o.status.code(), Some(1), "stdout: {}\nstderr: {}", stdout(o), stderr(o));
    let err = stderr(o);
    for n in needles {
        assert!(err.contains(n), "stderr lacks {n:?}:\n{err}");
    }
}

const HELPERS: &str = "fn double(x) { return __twice(x) }\n\
                       fn __twice(x) { return x * 2 }\n\
                       fn greet(n) { return \"hi \" + n }\n\
                       fn main() { io.print(\"helpers main ran\") }\n";

#[test]
fn qualified_call() {
    let d = tree(&[
        ("helpers.crush", HELPERS),
        ("main.crush", "import helpers\nfn main() { io.print(helpers.double(21)) }\n"),
    ]);
    assert_prints(&run_in(d.path(), &["main.crush"]), &["42"]);
}

#[test]
fn selective_call() {
    let d = tree(&[
        ("helpers.crush", HELPERS),
        ("main.crush", "import helpers { double, greet }\nio.print(double(5))\nio.print(greet(\"ann\"))\n"),
    ]);
    assert_prints(&run_in(d.path(), &["main.crush"]), &["10", "hi ann"]);
}

#[test]
fn alias_of_dotted_path() {
    let d = tree(&[
        ("util/strings.crush", "fn title(s) { return \"[\" + s + \"]\" }\n"),
        ("main.crush", "import util.strings as s\nio.print(s.title(\"x\"))\n"),
    ]);
    assert_prints(&run_in(d.path(), &["main.crush"]), &["[x]"]);
}

#[test]
fn dotted_path_without_alias_binds_its_last_segment() {
    let d = tree(&[
        ("util/strings.crush", "fn title(s) { return \"[\" + s + \"]\" }\n"),
        ("main.crush", "import util.strings\nio.print(strings.title(\"y\"))\n"),
    ]);
    assert_prints(&run_in(d.path(), &["main.crush"]), &["[y]"]);
}

#[test]
fn same_function_name_in_two_modules() {
    let d = tree(&[
        ("a.crush", "fn name() { return \"a\" }\n"),
        ("b.crush", "fn name() { return \"b\" }\n"),
        ("main.crush", "import a\nimport b\nfn name() { return \"local\" }\nio.print(a.name() + b.name() + name())\n"),
    ]);
    assert_prints(&run_in(d.path(), &["main.crush"]), &["ablocal"]);
}

#[test]
fn selective_collision_with_another_selective_import_is_an_error() {
    let d = tree(&[
        ("a.crush", "fn f() { return 1 }\n"),
        ("b.crush", "fn f() { return 2 }\n"),
        ("main.crush", "import a { f }\nimport b { f }\nio.print(f())\n"),
    ]);
    assert_fails(
        &run_in(d.path(), &["main.crush"]),
        &["main.crush:2:1", "`import b { f }` collides with `import a { f }` (line 1)"],
    );
}

#[test]
fn selective_collision_with_a_local_function_is_an_error() {
    let d = tree(&[
        ("helpers.crush", HELPERS),
        ("main.crush", "import helpers { double }\nfn double(x) { return x }\nio.print(double(1))\n"),
    ]);
    assert_fails(
        &run_in(d.path(), &["main.crush"]),
        &["main.crush:1:1", "`import helpers { double }` collides with `fn double` (line 2)"],
    );
}

#[test]
fn module_calls_its_own_helpers_unqualified() {
    // `double` calls `__twice` bare; a local of the same name shadows it.
    let d = tree(&[
        (
            "m.crush",
            "fn __twice(x) { return x * 2 }\n\
             fn double(x) { return __twice(x) }\n\
             fn shadow(__twice) { return __twice + 1 }\n",
        ),
        ("main.crush", "import m as m { double }\nio.print(double(4))\nio.print(m.shadow(4))\n"),
    ]);
    assert_prints(&run_in(d.path(), &["main.crush"]), &["8", "5"]);
}

#[test]
fn diamond_imports_load_one_copy() {
    let d = tree(&[
        ("base.crush", "fn base() { return 7 }\n"),
        ("left.crush", "import base\nfn left() { return base.base() }\n"),
        ("right.crush", "import base { base }\nfn right() { return base() + 1 }\n"),
        ("top.crush", "import left\nimport right\nio.print(left.left() + right.right())\n"),
    ]);
    assert_prints(&run_in(d.path(), &["top.crush"]), &["15"]);
    let casm = Command::new(bin("crushc"))
        .args(["top.crush", "--emit", "cvms", "-o", "-"])
        .current_dir(d.path())
        .output()
        .unwrap();
    assert!(casm.status.success(), "{}", stderr(&casm));
    let text = stdout(&casm);
    assert_eq!(text.matches(".func base::base").count(), 1, "{text}");
}

#[test]
fn import_cycle_is_an_error_showing_the_cycle() {
    let d = tree(&[
        ("ca.crush", "import cb\nfn a() { return 1 }\n"),
        ("cb.crush", "import ca\nfn b() { return 2 }\n"),
        ("main.crush", "import ca\nio.print(1)\n"),
    ]);
    assert_fails(
        &run_in(d.path(), &["main.crush"]),
        &["cb.crush:1:1", "import cycle: ca.crush → cb.crush → ca.crush"],
    );
}

#[test]
fn module_not_found_lists_every_directory_searched() {
    let d = tree(&[("main.crush", "import nosuch.thing\nio.print(1)\n"), ("lib/.keep", "")]);
    let o = run_env(d.path(), &["main.crush", "-L", "lib"], Some("/nonexistent-crush-path"));
    assert_fails(
        &o,
        &[
            "main.crush:1:1",
            "module not found — no nosuch/thing.crush in any of:",
            "  . (",
            "  lib (",
            "  /nonexistent-crush-path (does not exist)",
        ],
    );
}

#[test]
fn lib_path_and_crush_path_are_searched_in_order() {
    let d = tree(&[
        ("ext1/shared.crush", "fn who() { return \"ext1\" }\n"),
        ("ext2/shared.crush", "fn who() { return \"ext2\" }\n"),
        ("ext2/only2.crush", "fn bye() { return \"via CRUSH_PATH\" }\n"),
        ("main.crush", "import shared\nimport only2 { bye }\nio.print(shared.who())\nio.print(bye())\n"),
    ]);
    // -L comes before CRUSH_PATH: ext1's `shared` wins.
    let o = run_env(d.path(), &["main.crush", "-L", "ext1"], Some("ext2"));
    assert_prints(&o, &["ext1", "via CRUSH_PATH"]);
    // CRUSH_PATH alone.
    let o = run_env(d.path(), &["main.crush"], Some("ext2"));
    assert_prints(&o, &["ext2", "via CRUSH_PATH"]);
    // crushc takes -L too.
    let o = Command::new(bin("crushc"))
        .args(["main.crush", "-L", "ext1", "-L", "ext2", "-o", "out.cvm1"])
        .current_dir(d.path())
        .env_remove("CRUSH_PATH")
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert_prints(&run_in(d.path(), &["out.cvm1"]), &["ext1", "via CRUSH_PATH"]);
}

#[test]
fn imported_main_is_ignored() {
    let d = tree(&[
        ("helpers.crush", HELPERS),
        ("main.crush", "import helpers\nio.print(helpers.greet(\"z\"))\n"),
    ]);
    let o = run_in(d.path(), &["main.crush"]);
    assert_prints(&o, &["hi z"]);
    assert!(!stdout(&o).contains("helpers main ran"));
}

#[test]
fn top_level_statement_in_a_module_is_rejected() {
    let d = tree(&[
        ("m.crush", "fn ok() { return 1 }\nio.print(\"init\")\n"),
        ("main.crush", "import m\nio.print(m.ok())\n"),
    ]);
    assert_fails(
        &run_in(d.path(), &["main.crush"]),
        &["m.crush:2:", "a top-level statement in imported module `m`", "no init code"],
    );
    let d = tree(&[
        ("m.crush", "fn ok() { return 1 }\nlet x = 5\n"),
        ("main.crush", "import m\nio.print(m.ok())\n"),
    ]);
    assert_fails(&run_in(d.path(), &["main.crush"]), &["m.crush:2:", "a top-level `let`"]);
}

#[test]
fn error_inside_an_imported_file_names_that_file_and_line() {
    let d = tree(&[
        ("errmod.crush", "fn fine() { return 1 }\nfn broken(x) {\n    return x + undefined_thing\n}\n"),
        ("main.crush", "import errmod\nio.print(errmod.broken(1))\n"),
    ]);
    assert_fails(
        &run_in(d.path(), &["main.crush"]),
        &["errmod.crush:3:16: Undefined variable: undefined_thing"],
    );
    // The JSON record carries the module's file, not the compiled one.
    let o = Command::new(bin("crushc"))
        .args(["main.crush", "--message-format", "json"])
        .current_dir(d.path())
        .output()
        .unwrap();
    let err = stderr(&o);
    assert!(err.contains(r#""file":"errmod.crush","line":3,"col":16"#), "{err}");
    // A syntax error in the module: E-IM01 in the module's file.
    let d = tree(&[
        ("bad.crush", "fn ok() { return 1 }\nfn broken( {\n"),
        ("main.crush", "import bad\nio.print(bad.ok())\n"),
    ]);
    let o = Command::new(bin("crushc"))
        .args(["main.crush", "--message-format", "json"])
        .current_dir(d.path())
        .output()
        .unwrap();
    let err = stderr(&o);
    assert!(err.contains(r#""code":"E-IM01","level":"error","file":"bad.crush","line":2"#), "{err}");
}

#[test]
fn unknown_qualified_name_is_an_error() {
    let d = tree(&[
        ("helpers.crush", HELPERS),
        ("main.crush", "import helpers\nio.print(helpers.nope(1))\n"),
    ]);
    assert_fails(
        &run_in(d.path(), &["main.crush"]),
        &["main.crush:2:", "`helpers.nope`: module `helpers`", "defines no function `nope` (it defines: double, greet)"],
    );
}

#[test]
fn capability_check_sees_imported_code_by_reachability() {
    let fsmod = "fn reach() { return fs.read(\"x.txt\") }\n\
                 fn unused() { return fs.write(\"y\", \"z\") }\n\
                 fn plain() { return 1 }\n";
    let d = tree(&[
        ("fsmod.crush", fsmod),
        ("reach.crush", "import fsmod\nio.print(fsmod.reach())\n"),
        ("unreach.crush", "import fsmod\nio.print(fsmod.plain())\n"),
    ]);
    let o = run_in(d.path(), &["reach.crush"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("--fs: fs.read"), "{}", stderr(&o));
    assert!(!stderr(&o).contains("fs.write"), "unreachable fs.write must not be required: {}", stderr(&o));
    assert_prints(&run_in(d.path(), &["unreach.crush"]), &["1"]);
}

#[test]
fn external_and_mcp_imports_are_refused_at_compile_time() {
    let d = tree(&[
        ("http.crush", "import @http \"https://example.com/x.json\" as data\nio.print(1)\n"),
        ("mcp.crush", "use @mcp \"https://mcp.example\" { \"t\" } as m\nio.print(1)\n"),
    ]);
    for (file, needle) in [
        ("http.crush", "`import @http \"https://example.com/x.json\"` is not supported yet"),
        ("mcp.crush", "`use @mcp \"https://mcp.example\"` is not supported yet"),
    ] {
        let o = Command::new(bin("crushc"))
            .args([file, "--check"])
            .current_dir(d.path())
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(1), "{file}: {}", stderr(&o));
        assert!(stderr(&o).contains(needle), "{file}: {}", stderr(&o));
        // Nothing reaches the VM: no output from the program.
        let o = run_in(d.path(), &[file]);
        assert_fails(&o, &[needle]);
        assert!(stdout(&o).is_empty());
    }
}

#[test]
fn struct_and_impl_in_a_module() {
    let d = tree(&[
        (
            "geo.crush",
            "struct Point { x: int, y: int }\n\
             impl Point {\n    fn norm2(self) { return __sq(self.x) + __sq(self.y) }\n}\n\
             fn __sq(v) { return v * v }\n\
             fn origin() { return Point { x: 3, y: 4 } }\n",
        ),
        ("main.crush", "import geo\nlet p = geo.origin()\nio.print(p.norm2())\n"),
    ]);
    assert_prints(&run_in(d.path(), &["main.crush"]), &["25"]);
}

/// A facet-shaped library (flat public functions plus `__`-prefixed helpers,
/// reading `sys.args()` and exiting 2 on a bad flag) used both ways.
#[test]
fn argparse_style_library_selective_and_qualified() {
    let lib = "fn __has(xs, v) {\n    for x in xs { if x == v { return true } }\n    return false\n}\n\
               fn cli_help(spec) { return \"Usage: \" + spec.name + \" [--help]\" }\n\
               fn cli_parse(spec) {\n    let argv = sys.args()\n    if __has(argv, \"--help\") {\n        io.print(cli_help(spec))\n        sys.exit(0)\n    }\n    for a in argv {\n        if a != \"--ok\" {\n            io.print(\"unknown option \" + a)\n            sys.exit(2)\n        }\n    }\n    return { ok: true }\n}\n";
    let d = tree(&[
        ("cli.crush", lib),
        ("sel.crush", "import cli { cli_parse, cli_help }\nlet a = cli_parse({ name: \"demo\" })\nio.print(\"parsed\")\n"),
        ("qual.crush", "import cli\nlet a = cli.cli_parse({ name: \"demo\" })\nio.print(\"parsed\")\n"),
    ]);
    for file in ["sel.crush", "qual.crush"] {
        let o = run_in(d.path(), &[file, "--", "--help"]);
        assert_eq!(o.status.code(), Some(0), "{file}: {}", stderr(&o));
        assert!(stdout(&o).contains("Usage: demo [--help]"), "{file}: {}", stdout(&o));
        let o = run_in(d.path(), &[file, "--", "--bogus"]);
        assert_eq!(o.status.code(), Some(2), "{file}: {}", stderr(&o));
        assert!(stdout(&o).contains("unknown option --bogus"), "{file}: {}", stdout(&o));
        assert_prints(&run_in(d.path(), &[file, "--", "--ok"]), &["parsed"]);
    }
}

#[test]
fn repl_keeps_imports_across_lines() {
    let d = tree(&[("helpers.crush", HELPERS)]);
    let mut child = Command::new(bin("crush-repl"))
        .current_dir(d.path())
        .env_remove("CRUSH_PATH")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write as _;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"import helpers\nhelpers.double(4)\nimport helpers { greet }\ngreet(\"r\")\n")
        .unwrap();
    let o = child.wait_with_output().unwrap();
    // Results go to stderr, the prompt to stdout; check both.
    let out = stdout(&o) + &stderr(&o);
    assert!(out.contains("=> 8"), "{out}");
    assert!(out.contains("=> hi r"), "{out}");
    assert!(!out.contains("helpers::"), "imported functions are not listed as defined: {out}");
}
