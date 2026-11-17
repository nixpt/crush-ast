//! GitHub issue #544: `async fn main` as the entry point silently did nothing.
//!
//! An `async fn` is lowered to a wrapper that spawns its body as a task and
//! returns the handle. For the entry point nobody awaits that handle, and a
//! task never yielded to is abandoned when `main` returns (by design), so
//! `async fn main() { print(5) }` printed nothing and exited 0. Top-level
//! statements are merged into `main`'s body, so they were dropped too.
//!
//! The entry point is now refused at compile time with a message naming the
//! form. Being refused in the frontend, it is refused for every engine.

use std::process::Command;

fn compile_err(source: &str) -> String {
    match crush_lang_sdk::compile::compile_crush_source(source) {
        Ok(_) => panic!("expected a compile error for:\n{source}"),
        Err(e) => format!("{e:#}"),
    }
}

const REFUSAL: &str = "`async fn main` cannot be the entry point";

/// The issue's repro. The location is the `fn` keyword's, as for any function.
#[test]
fn async_fn_main_is_refused() {
    let e = compile_err("async fn main() { print(5) }\n");
    assert!(e.contains(REFUSAL), "{e}");
    assert!(e.contains("line 1, col 7"), "{e}");
}

/// Top-level statements next to `async fn main` used to be merged into the
/// spawned body and dropped with it.
#[test]
fn async_fn_main_next_to_top_level_statements_is_refused() {
    let e = compile_err("print(1)\nasync fn main() {\n    print(5)\n}\n");
    assert!(e.contains(REFUSAL), "{e}");
    assert!(e.contains("line 2, col 7"), "{e}");
}

/// `async fn` other than the entry point is unchanged: a plain `fn main`
/// awaits it.
#[test]
fn other_async_fns_still_run() {
    let program = crush_lang_sdk::compile::compile_crush_source(
        "async fn sub(a, b) {\n    return a - b\n}\nfn main() {\n    print(await sub(10, 3))\n}\n",
    )
    .expect("compiles");
    let out = crush_lang_sdk::Runtime::new()
        .with_host_caps(crush_lang_sdk::HostCapsBuilder::new().build())
        .run(&program)
        .expect("runs")
        .output;
    assert_eq!(out, "7\n");
}

/// A function that is merely named like an async helper does not trip the
/// refusal; only the entry point does.
#[test]
fn an_async_helper_with_a_sync_main_is_not_refused() {
    crush_lang_sdk::compile::compile_crush_source(
        "async fn main_loop() { print(1) }\nfn main() { await main_loop() }\n",
    )
    .expect("compiles");
}

/// End to end through `crush-run`: a non-zero exit and the message on stderr,
/// never a silent exit 0.
#[test]
fn crush_run_refuses_async_fn_main_loudly() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("a.crush");
    std::fs::write(&src, "async fn main() { print(5) }\n").unwrap();
    let bin = option_env!("CARGO_BIN_EXE_crush-run").unwrap_or("crush-run");
    let out = Command::new(bin)
        .args(["run", src.to_str().unwrap()])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run crush-run");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "exit 0; stderr: {stderr}");
    assert!(out.stdout.is_empty(), "stdout: {:?}", out.stdout);
    assert!(stderr.contains(REFUSAL), "{stderr}");
}
