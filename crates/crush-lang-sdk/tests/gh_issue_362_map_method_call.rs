//! GitHub issue #362 / CRUSH-362: `h.add(41)` on a local map
//! (`{ add: |a| => a + 1 }`) compiled to a host-capability call named after
//! the field ("not available in this host: add"). A member call on a map now
//! resolves at run time: it calls the function stored under the key.

use crush_lang_sdk::{HostCapsBuilder, Runtime};

fn run(source: &str) -> Result<String, String> {
    let program = crush_lang_sdk::compile::compile_crush_source(source)
        .unwrap_or_else(|e| panic!("compile: {e:#}\n{source}"));
    Runtime::new()
        .with_host_caps(HostCapsBuilder::new().build())
        .run(&program)
        .map(|r| r.output)
        .map_err(|e| e.to_string())
}

/// The issue's repro.
#[test]
fn the_issue_repro() {
    let out = run("fn main() {\n    let h = { add: |a| => a + 1 }\n    print(h.add(41))\n}\n")
        .expect("runs");
    assert_eq!(out, "42\n");
}

/// A captured lambda stored in a map is called with its captures as the
/// leading parameters, exactly as `CALL_VALUE` would call it.
#[test]
fn a_captured_lambda_stored_in_a_map_sees_its_capture() {
    let out = run(
        "fn main() {\n    let k = 10\n    let h = { sub: |a| => k - a }\n    print(h.sub(4))\n}\n",
    )
    .expect("runs");
    assert_eq!(out, "6\n");
}

/// The struct method form (CRUSH-277 S5) is unchanged: resolved on the
/// receiver's Kind, not through the map path.
#[test]
fn the_struct_method_form_is_unchanged() {
    let out = run(
        "struct Point { x: int, y: int }\nimpl Point {\n    fn norm2(self) { return self.x * self.x + self.y * self.y }\n}\nfn main() {\n    let p = Point { x: 3, y: 4 }\n    print(p.norm2())\n}\n",
    )
    .expect("runs");
    assert_eq!(out, "25\n");
}

/// Decided semantics for a member call whose receiver has no function under
/// the key: a loud runtime error, never a silent capability request and never
/// a silent `null`.
#[test]
fn a_missing_map_key_is_a_clear_error() {
    let e = run("fn main() {\n    let h = { add: |a| => a + 1 }\n    print(h.sub(41))\n}\n")
        .expect_err("errors");
    assert!(e.contains("map has no member `sub`"), "{e}");
}

/// A key that holds a non-function value is also a clear runtime error
/// (the same check `CALL_VALUE` applies).
#[test]
fn a_map_field_not_holding_a_function_is_a_clear_error() {
    let e = run("fn main() {\n    let h = { n: 7 }\n    print(h.n())\n}\n").expect_err("errors");
    assert!(e.contains("int is not callable"), "{e}");
}

/// Method-call syntax on a non-struct, non-map receiver is refused with a
/// clear type error.
#[test]
fn a_member_call_on_other_types_is_a_clear_error() {
    let e = run("fn main() {\n    let x = 5\n    print(x.foo())\n}\n").expect_err("errors");
    assert!(
        e.contains("member calls resolve on struct instances (COSM Records) and maps"),
        "{e}"
    );
}

/// Genuine host capability calls are unchanged: `fs.read` is still gated
/// behind `--fs`, and its name is still inferred from the bytecode.
#[test]
fn capability_calls_are_unchanged() {
    let program = crush_lang_sdk::compile::compile_crush_source(
        "fn main() {\n    let s = fs.read(\"x.txt\")\n    print(s)\n}\n",
    )
    .expect("compiles");
    let used = crush_vm::capabilities_used(&program).expect("decode");
    assert!(used.contains("fs.read"), "{used:?}");
    let missing = Runtime::new().missing_grants(&program).expect("preflight");
    assert!(!missing.is_empty(), "{missing:?}");
    let e = Runtime::new().run(&program).expect_err("refused");
    assert!(e.to_string().contains("fs.read"), "{e}");
}
