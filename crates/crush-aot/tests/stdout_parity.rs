//! What AOT-compiled programs *print*, on the Rust and C backends.
//!
//! `differential_aot.rs` compares only `main`'s return value, so bugs on the print path
//! (garbled strings, float formatting, `io.print`'s stack contract) passed it (CRUSH-216,
//! CRUSH-220). Each expected output below is what `crush-run` prints for the same source.

use std::path::{Path, PathBuf};
use std::process::Command;

use crush_aot::AotCompiler;

fn aot_runner() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_crush-aot-runner"))
}

fn cc_available(cc: &str) -> bool {
    Command::new(cc).arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

/// Program stdout, without the type-tagged result line the runner prints last.
fn program_stdout(so: &Path) -> String {
    let out = Command::new(aot_runner()).arg(so).output().expect("spawn crush-aot-runner");
    assert!(
        out.status.success(),
        "crush-aot-runner failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).expect("utf-8 stdout");
    let body = text.trim_end_matches('\n');
    match body.rfind('\n') {
        Some(i) => format!("{}\n", &body[..i]),
        None => String::new(),
    }
}

fn assert_prints(name: &str, source: &str, expected: &str) {
    let program = crush_frontend::compile_crush_source(source).expect("frontend");
    let compiler = AotCompiler::new();

    let so = compiler.compile_casm(&program, &format!("{name}_rust")).expect("rust backend");
    assert_eq!(program_stdout(&so), expected, "rust backend: {name}");

    if cc_available("gcc") {
        let so = compiler.compile_c(&program, &format!("{name}_c"), "gcc").expect("c backend");
        assert_eq!(program_stdout(&so), expected, "c backend: {name}");
    } else {
        eprintln!("gcc not found; skipping the C backend for {name}");
    }
}

// CRUSH-216: one 256-byte ring buffer held every C string, so a string stored in a
// variable was overwritten by later string work (`keep7` printed `gh36`).
#[test]
fn stored_string_survives_later_string_work() {
    assert_prints(
        "stored_string",
        r#"fn tag(n) { return "keep" + n; }
           fn main() {
               let keep = tag(7);
               let s = "";
               let i = 0;
               while i < 40 { s = "abcdefgh" + i; i = i + 1; }
               io.print(keep);
               io.print(s);
               return 0;
           }"#,
        "keep7\nabcdefgh39\n",
    );
}

// CRUSH-216: a string past 256 bytes came back truncated (len 32), with no error.
#[test]
fn long_string_is_not_truncated() {
    assert_prints(
        "long_string",
        r#"fn main() {
               let s = "";
               let i = 0;
               while i < 30 { s = s + "0123456789"; i = i + 1; }
               io.print(len(s));
               io.print(s == s + "");
               return 0;
           }"#,
        "300\ntrue\n",
    );
}

// The game_of_life render shape: rows built in nested loops, joined with "\n".
#[test]
fn board_built_in_nested_loops() {
    assert_prints(
        "board",
        r##"fn grid(n) {
               let out = "";
               let r = 0;
               while r < n {
                   let line = "";
                   let c = 0;
                   while c < n {
                       let ch = ".";
                       if c == r { ch = "#"; }
                       line = line + ch;
                       c = c + 1;
                   }
                   out = out + line + "\n";
                   r = r + 1;
               }
               io.print(out);
               return;
           }
           fn main() { grid(4); io.print("x=" + 5); return 0; }"##,
        "#...\n.#..\n..#.\n...#\n\nx=5\n",
    );
}

// CRUSH-216: C `io.print` pushed nothing while the CASM pops its result, so the pop
// took the caller's pending operand: `f() + f()` became a type error.
#[test]
fn print_inside_an_expression_keeps_the_stack() {
    assert_prints(
        "print_stack",
        r#"fn f() { io.print("call"); return 1; }
           fn main() { let a = f() + f(); io.print(a); return 0; }"#,
        "call\ncall\n2\n",
    );
}

// CRUSH-216: C printed floats with `%g` (`1`, `0.3`, `1.5e+10`) and concatenated them
// with `%.15g`; `1e20` was emitted as an integer literal too large for C.
#[test]
fn floats_print_like_the_vm() {
    assert_prints(
        "floats",
        r#"fn main() {
               io.print(1.0);
               io.print(0.1 + 0.2);
               io.print(2.0 / 3.0);
               io.print(15000000000.0);
               io.print(100000000000000000000.0);
               io.print(0.0000001);
               io.print(-2.5);
               io.print("x=" + (0.1 + 0.2));
               return 0;
           }"#,
        "1.0\n0.30000000000000004\n0.6666666666666666\n15000000000.0\n\
         100000000000000000000.0\n0.0000001\n-2.5\nx=0.30000000000000004\n",
    );
}

// CRUSH-216: `conv.chr` returned one of 16 rotating static buffers, so a stored result
// was overwritten after 16 more calls.
#[test]
fn stored_conv_chr_result_survives() {
    assert_prints(
        "conv_chr",
        r#"fn main() {
               let a = conv.chr(65);
               let z = "";
               let i = 0;
               while i < 20 { z = conv.chr(66 + i); i = i + 1; }
               io.print(a + z);
               return 0;
           }"#,
        "AU\n",
    );
}
