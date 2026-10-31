//! CRUSH-229: the float-to-text rule lives in `crush_vm::float_text`; Rust AOT embeds
//! that source, but generated C has its own hand-ported `_float_text`. This runs a
//! wide set of floats through the VM (the oracle) and both AOT backends and requires
//! byte-identical output, so a change to any one copy fails here.
//!
//! The floats are built with arithmetic (no exponent literals) so the program only
//! depends on printing.

use std::path::Path;
use std::process::{Command, Stdio};

use crush_aot::AotCompiler;
use crush_lang_sdk::{Quotas, Runtime};

fn cc_available() -> bool {
    Command::new("gcc").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

fn vm_stdout(source: &str) -> String {
    let program = crush_lang_sdk::compile::compile_crush_source(source).expect("frontend");
    let quotas = Quotas { max_steps: 50_000_000, ..Quotas::default() };
    let result = Runtime::with_quotas(quotas).run(&program).expect("vm run");
    assert!(result.halted, "vm did not halt");
    result.output
}

/// Program stdout without the type-tagged result line `crush-aot-runner` prints last.
fn aot_stdout(so: &Path) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_crush-aot-runner"))
        .arg(so)
        .stdin(Stdio::null())
        .output()
        .expect("spawn crush-aot-runner");
    assert!(out.status.success(), "runner failed: {}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8(out.stdout).expect("utf-8");
    let body = text.trim_end_matches('\n');
    match body.rfind('\n') {
        Some(i) => format!("{}\n", &body[..i]),
        None => String::new(),
    }
}

fn first_diff(a: &str, b: &str) -> String {
    for (i, (x, y)) in a.lines().zip(b.lines()).enumerate() {
        if x != y {
            return format!("line {}: vm `{x}` vs `{y}`", i + 1);
        }
    }
    format!("line counts differ: vm {} vs {}", a.lines().count(), b.lines().count())
}

/// Every float is printed bare (`io.print(x)`) and concatenated (`"v=" + x`).
const PROGRAM: &str = r#"
fn show(x) { io.print(x); io.print("v=" + x); }
fn pow10(n) { let r = 1.0; let i = 0; while i < n { r = r * 10.0; i = i + 1; } return r; }
fn pow2(n) { let r = 1.0; let i = 0; while i < n { r = r * 2.0; i = i + 1; } return r; }
fn inv_pow2(n) { let r = 1.0; let i = 0; while i < n { r = r / 2.0; i = i + 1; } return r; }
fn main() {
    show(0.0);
    show(1.0);
    show(-1.0);
    show(0.0 * -1.0);
    show(0.1 + 0.2);
    show(0.1 * 3.0);
    show(1.0 / 3.0);
    show(2.0 / 3.0);
    show(100.0 / 7.0);
    show(4.35);
    show(-1234.5);
    show(0.000123);
    show(123456789.12345678);
    show(9007199254740992.0);
    show(9007199254740993.0);
    show(pow10(15) + 0.3);
    show(pow10(16));
    // Exact ties at the last shortest digit: glibc rounds half-even, Rust half-up.
    show(pow10(15) + 0.125);
    show(pow10(15) + 0.375);
    show(pow10(15) + 0.625);
    show(pow10(15) + 0.875);
    show(pow2(51) + 0.25);
    show(pow2(51) + 0.75);
    show(pow2(52) + 0.5);
    show(pow10(20));
    show(pow10(22));
    show(pow10(23));
    show(pow10(300));
    show(1.0 / pow10(7));
    show(1.0 / pow10(20));
    show(1.0 / pow10(300));
    show(pow2(52));
    show(pow2(53));
    show(pow2(63));
    show(pow2(64));
    show(pow2(1023));
    show(pow2(1023) * 1.5);
    show(inv_pow2(1022));
    show(inv_pow2(1023));
    show(inv_pow2(1074));
    show(inv_pow2(1074) * 3.0);
    show(-inv_pow2(1074));
    let inf = pow2(1023) * 4.0;
    show(inf);
    show(0.0 - inf);
    show(inf - inf);
    show(inf * 0.0);
    // Sweeps: long, unround digit strings across the whole exponent range.
    let up = 1.1;
    let n = 0;
    while n < 1500 { up = up * 1.7; show(up); n = n + 1; }
    let down = 1.1;
    n = 0;
    while n < 1500 { down = down / 1.7; show(down); n = n + 1; }
    let k = 0;
    while k < 400 { show(k / 7.0); show(0.0 - k / 13.0); k = k + 1; }
    return 0;
}
"#;

#[test]
fn float_text_is_identical_on_vm_rust_aot_and_c_aot() {
    let source = PROGRAM;
    let oracle = vm_stdout(source);
    assert!(oracle.lines().count() > 6000, "oracle printed {} lines", oracle.lines().count());
    // The oracle really contains the rule's special cases.
    for needle in ["NaN", "\ninf\n", "\n-inf\n", "\n-0.0\n", "100000000000000000000.0\n"] {
        assert!(oracle.contains(needle), "vm output lacks {needle:?}");
    }

    let program = crush_frontend::compile_crush_source(source).expect("frontend");
    let compiler = AotCompiler::new();
    let so = compiler.compile_casm(&program, "float_text_rust").expect("rust backend");
    let rust = aot_stdout(&so);
    assert!(rust == oracle, "rust AOT differs from the VM: {}", first_diff(&oracle, &rust));

    if cc_available() {
        let so = compiler.compile_c(&program, "float_text_c", "gcc").expect("c backend");
        let c = aot_stdout(&so);
        assert!(c == oracle, "C AOT differs from the VM: {}", first_diff(&oracle, &c));
    } else {
        eprintln!("gcc not found; skipping the C backend");
    }
}
