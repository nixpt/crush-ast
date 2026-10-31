//! CRUSH-368: `crush-run --codebase FILE_OR_DIR` (and `crush-repl --codebase`)
//! serve the `codebase.*` capabilities over the given source files. The flag is
//! the grant: without it the program is refused up front, naming the flag.

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn bin(name: &str) -> String {
    match name {
        "crush-run" => option_env!("CARGO_BIN_EXE_crush-run").unwrap_or("crush-run").to_string(),
        _ => option_env!("CARGO_BIN_EXE_crush-repl").unwrap_or("crush-repl").to_string(),
    }
}
fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

/// Two small modules: `util.double` is called from `app.main`'s helper.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(src.join("util.crush"), "fn double(x) { return x * 2 }\n").unwrap();
    std::fs::write(
        src.join("app.crush"),
        "fn run_it(n) { return double(n) + 1 }\nfn main() { return run_it(3) }\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("query.crush"),
        "fn main() {\n  print(codebase.modules())\n  print(codebase.callers(\"double\"))\n}\n",
    )
    .unwrap();
    dir
}

#[test]
fn files_are_queryable() {
    let d = project();
    let (a, b) = (d.path().join("src/util.crush"), d.path().join("src/app.crush"));
    let o = Command::new(bin("crush-run"))
        .args(["run", d.path().join("query.crush").to_str().unwrap()])
        .args(["--codebase", a.to_str().unwrap(), "--codebase", b.to_str().unwrap()])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let t = text(&o);
    assert!(o.status.success(), "{t}");
    assert!(t.contains("app") && t.contains("util"), "modules: {t}");
    assert!(t.contains("run_it"), "callers of double: {t}");
}

#[test]
fn a_directory_expands_to_its_crush_files() {
    let d = project();
    let o = Command::new(bin("crush-run"))
        .args(["run", d.path().join("query.crush").to_str().unwrap()])
        .args(["--codebase", d.path().join("src").to_str().unwrap()])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let t = text(&o);
    assert!(o.status.success(), "{t}");
    assert!(t.contains("run_it"), "{t}");
}

#[test]
fn without_the_flag_it_is_refused_naming_the_grant() {
    let d = project();
    let q = d.path().join("query.crush");
    let o = Command::new(bin("crush-run")).args(["run", q.to_str().unwrap()]).stdin(Stdio::null()).output().unwrap();
    assert!(!o.status.success());
    assert!(text(&o).contains("--codebase FILE"), "{}", text(&o));
    // `caps` names the same grant.
    let o = Command::new(bin("crush-run")).args(["caps", q.to_str().unwrap()]).output().unwrap();
    assert!(text(&o).contains("--codebase FILE"), "{}", text(&o));
}

#[test]
fn bad_inputs_are_loud() {
    let d = project();
    let q = d.path().join("query.crush");
    let o = Command::new(bin("crush-run"))
        .args(["run", q.to_str().unwrap(), "--codebase", "/nonexistent/x.crush"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!o.status.success() && text(&o).contains("--codebase"), "{}", text(&o));
    // Two files with the same module name.
    std::fs::create_dir(d.path().join("other")).unwrap();
    std::fs::write(d.path().join("other/util.crush"), "fn triple(x) { return x * 3 }\n").unwrap();
    let o = Command::new(bin("crush-run"))
        .args(["run", q.to_str().unwrap()])
        .args(["--codebase", d.path().join("src").to_str().unwrap()])
        .args(["--codebase", d.path().join("other").to_str().unwrap()])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!o.status.success() && text(&o).contains("already taken"), "{}", text(&o));
}

#[test]
fn the_repl_takes_the_flag_too() {
    let d = project();
    let mut child = Command::new(bin("crush-repl"))
        .args(["--codebase", d.path().join("src").to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"print(codebase.modules())\n")
        .unwrap();
    let o = child.wait_with_output().unwrap();
    let t = text(&o);
    assert!(t.contains("util") && t.contains("app"), "{t}");
}
