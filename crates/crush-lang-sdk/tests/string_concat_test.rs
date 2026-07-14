//! `+` with a string operand — and the VM divergence it exposed.
//!
//! `print("Python says 5^3 is: " + result)` — a string plus a number — is the single most common
//! line anyone writes, and it was a hard type error. (`"a" + "b"` already worked; only the MIXED
//! case failed.) It blocked crush-website's example.crush.
//!
//! Fixing it surfaced something worse. There are THREE add implementations (scheduler.rs,
//! portable_vm.rs, fastvm). portable_vm's ADD did not guard its operands and leaned on
//! `to_f64_p`, which ended in `_ => 0.0`. So on the SAME source:
//!
//!     scheduler.rs   "a" + "b"   -> TypeError (loud)
//!     portable_vm    "a" + "b"   -> Int(0)    (silent, WRONG)
//!     portable_vm    "x: " + 5   -> Int(5)    (silent, WRONG)
//!
//! Two VMs, one program, different answers, no error. A silent miscompile — the same
//! `_ => 0.0` / `_ => push(TAG_NULL)` disease this codebase keeps shipping.
//!
//! These pin the semantics so the engines cannot drift apart again.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::process::Command;

fn crush_run_bin() -> &'static str {
    option_env!("CARGO_BIN_EXE_crush-run").unwrap_or("crush-run")
}

fn run(src: &str) -> String {
    let dir = std::env::temp_dir().join(format!("crush_cat_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // Unique per source: cargo runs these tests in PARALLEL, and a shared filename means they
    // overwrite each other's program mid-run. (It bit me writing this file.)
    let mut h = DefaultHasher::new();
    src.hash(&mut h);
    let f = dir.join(format!("t{}.crush", h.finish()));
    write!(std::fs::File::create(&f).unwrap(), "{src}").unwrap();
    let out = Command::new(crush_run_bin())
        .args(["run", f.to_str().unwrap()])
        .output()
        .expect("crush-run");
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    // strip the VM's trailing "[steps=..]" telemetry line
    s.lines()
        .filter(|l| !l.starts_with("[steps="))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

#[test]
fn string_plus_int() {
    assert_eq!(run(r#"fn main() { print("x: " + 5); }"#), "x: 5");
}

#[test]
fn int_plus_string() {
    assert_eq!(run(r#"fn main() { print(5 + " y"); }"#), "5 y");
}

#[test]
fn string_plus_float_keeps_fidelity() {
    assert_eq!(run(r#"fn main() { print("v: " + 1.5); }"#), "v: 1.5");
}

#[test]
fn string_plus_string_still_concatenates() {
    assert_eq!(run(r#"fn main() { print("a" + "b"); }"#), "ab");
}

#[test]
fn numeric_add_is_still_numeric() {
    // The regression that matters. `1 + 2` must be 3, NOT "12" — a string-first ADD arm that
    // caught too much would silently turn every arithmetic op into concatenation.
    assert_eq!(run(r#"fn main() { print(1 + 2); }"#), "3");
}

#[test]
fn float_add_is_still_numeric() {
    assert_eq!(run(r#"fn main() { print(1.5 + 2); }"#), "3.5");
}
