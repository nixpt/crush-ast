//! #258: `crushc` / `crush build` enforce `@capabilities` like
//! `crush-run run FILE.crush` does, and the declaration survives into the
//! `.cvm1` so `crush-run caps` reports it.

use std::process::{Command, Output};

fn crushc() -> &'static str {
    option_env!("CARGO_BIN_EXE_crushc").unwrap_or("crushc")
}
fn crush_run() -> &'static str {
    option_env!("CARGO_BIN_EXE_crush-run").unwrap_or("crush-run")
}
fn crush() -> &'static str {
    option_env!("CARGO_BIN_EXE_crush").unwrap_or("crush")
}
fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

const UNDECLARED: &str = "@capabilities [time.now_ms]\nprint(env.get(\"HOME\"))\n";
const DECLARED: &str = "@capabilities [env.get]\nprint(env.get(\"HOME\"))\n";

fn write(dir: &tempfile::TempDir, name: &str, src: &str) -> String {
    let p = dir.path().join(name);
    std::fs::write(&p, src).unwrap();
    p.to_str().unwrap().to_string()
}

#[test]
fn crushc_refuses_an_undeclared_capability() {
    let dir = tempfile::tempdir().unwrap();
    let src = write(&dir, "und.crush", UNDECLARED);
    let out_path = dir.path().join("und.cvm1");
    let o = Command::new(crushc()).args([&src, "-o", out_path.to_str().unwrap()]).output().unwrap();
    assert!(!o.status.success(), "{}", text(&o));
    let t = text(&o);
    assert!(t.contains("compile") && t.contains("not declared in @capabilities: env.get"), "{t}");
    assert!(!out_path.exists(), "no artifact on refusal");
}

#[test]
fn crush_build_and_check_and_json_refuse_too() {
    let dir = tempfile::tempdir().unwrap();
    let src = write(&dir, "und.crush", UNDECLARED);
    let o = Command::new(crush()).args(["build", &src]).output().unwrap();
    assert!(!o.status.success() && text(&o).contains("env.get"), "{}", text(&o));
    let o = Command::new(crushc()).args(["--check", &src]).output().unwrap();
    assert!(!o.status.success() && text(&o).contains("env.get"), "{}", text(&o));
    let o = Command::new(crushc()).args(["--message-format", "json", &src]).output().unwrap();
    assert!(!o.status.success());
    assert!(text(&o).contains("not declared in @capabilities: env.get"), "{}", text(&o));
}

#[test]
fn declared_program_compiles_and_keeps_the_declaration() {
    let dir = tempfile::tempdir().unwrap();
    let src = write(&dir, "ok.crush", DECLARED);
    let out_path = dir.path().join("ok.cvm1");
    let o = Command::new(crushc()).args([&src, "-o", out_path.to_str().unwrap()]).output().unwrap();
    assert!(o.status.success(), "{}", text(&o));
    let o = Command::new(crush_run()).args(["caps", out_path.to_str().unwrap()]).output().unwrap();
    let t = text(&o);
    assert!(t.contains("declares: env.get"), "{t}");
    // Same answer as asking about the source.
    let o = Command::new(crush_run()).args(["caps", &src]).output().unwrap();
    assert!(text(&o).contains("declares: env.get"), "{}", text(&o));
}

#[test]
fn programs_without_a_declaration_are_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let src = write(&dir, "plain.crush", "print(env.get(\"HOME\"))\n");
    let out_path = dir.path().join("plain.cvm1");
    let o = Command::new(crushc()).args([&src, "-o", out_path.to_str().unwrap()]).output().unwrap();
    assert!(o.status.success(), "{}", text(&o));
    let o = Command::new(crush_run()).args(["caps", out_path.to_str().unwrap()]).output().unwrap();
    assert!(text(&o).contains("no @capabilities declaration"), "{}", text(&o));
}
