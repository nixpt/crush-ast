//! COSM K1.4 found via `crush-run`'s preflight (after #222): `CALL_MEMBER` and
//! `CONSTRUCT` carry a member / Kind name the way `CAP_CALL` carries a
//! capability name, and capability inference took them for capabilities, so
//! `crush-run` refused `p.norm2()` ("not available in this host: norm2")
//! before running anything.

use std::process::Command;

const POINT: &str = r#"
struct Point { x: int, y: int }
impl Point {
    fn norm2(self) { return self.x * self.x + self.y * self.y }
    fn scaled(self, a, b) { return Point { x: self.x * a, y: self.y * b } }
    fn shout(self) { io.print("point!") }
}
fn main() {
    let p = Point { x: 3, y: 4 }
    print(p.norm2())
    print(p.scaled(2, 3).x)
    let q = new Point()
}
"#;

fn compile(src: &str) -> crush_vm::Program {
    crush_lang_sdk::compile::compile_crush_source(src).unwrap_or_else(|e| panic!("compile: {e:#}"))
}

#[test]
fn members_and_kinds_are_not_capabilities() {
    let used = crush_vm::capabilities_used(&compile(POINT)).expect("decode");
    for name in ["norm2", "scaled", "shout", "Point"] {
        assert!(!used.contains(name), "{name} reported as a capability: {used:?}");
    }
    // A capability used inside a method is still found.
    assert!(used.contains("io.print"), "{used:?}");
    let missing = crush_lang_sdk::Runtime::new().missing_grants(&compile(POINT)).expect("preflight");
    assert!(missing.is_empty(), "{missing:?}");
}

#[test]
fn a_capability_used_only_inside_a_method_is_still_required() {
    let src = r#"
struct F { path: str }
impl F { fn read(self) { return fs.cat(self.path) } }
fn main() {
    let f = F { path: "x" }
    return f.read()
}
"#;
    let used = crush_vm::capabilities_used(&compile(src)).expect("decode");
    assert!(used.contains("fs.cat"), "{used:?}");
    assert!(!used.contains("read"), "{used:?}");
}

#[test]
fn crush_run_runs_a_method_call() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("point.crush");
    std::fs::write(&src, POINT).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_crush-run")).args(["run", src.to_str().unwrap()]).output().expect("run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stdout: {stdout}\nstderr: {stderr}");
    assert_eq!(stdout, "25\n6\n");
}
