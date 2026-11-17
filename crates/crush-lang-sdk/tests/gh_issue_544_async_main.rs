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
    let program = crush_lang_sdk::compile::compile_crush_source(
        "async fn main_loop() { print(1) }\nfn main() { await main_loop() }\n",
    )
    .expect("compiles");
    assert_eq!(run(&program), "1\n");
}

fn run(program: &crush_vm::Program) -> String {
    crush_lang_sdk::Runtime::new()
        .with_host_caps(crush_lang_sdk::HostCapsBuilder::new().build())
        .run(program)
        .expect("runs")
        .output
}

/// Compile CAST that did not come from `compile_crush_source` (as a walker or
/// a hand-written file would hand it over) and run it.
fn compile_cast(cast: &crush_cast::Program) -> anyhow::Result<crush_vm::Program> {
    let casm = crush_frontend::compile_cast(cast)?;
    crush_lang_sdk::compile::casm_to_vm(&casm)
}

/// The check is on `main`, the function every engine runs, not on CAST
/// `entry`, which no engine reads (review of #554). CAST naming a sync
/// `entry` next to an async `main` is refused (it ran nothing before); CAST
/// naming an async `entry` next to a sync `main` runs `main`.
#[test]
fn the_check_is_on_main_not_on_cast_entry() {
    let mut cast =
        crush_frontend::parse_source("fn helper() { print(1) }\nasync fn main() { print(5) }\n")
            .expect("parses");
    cast.entry = "helper".to_string();
    let e = format!("{:#}", compile_cast(&cast).expect_err("refused"));
    assert!(e.contains(REFUSAL), "{e}");

    let mut cast =
        crush_frontend::parse_source("async fn sub() { print(1) }\nfn main() { print(5) }\n")
            .expect("parses");
    cast.entry = "sub".to_string();
    let program = compile_cast(&cast).expect("compiles");
    assert_eq!(run(&program), "5\n");
}

/// Walker CAST with an async `main` is refused too, and the message does not
/// tell a JavaScript author to write Crush.
#[cfg(feature = "polyglot-javascript")]
#[test]
fn walker_cast_with_an_async_main_is_refused() {
    let cast = crush_lang_js::js_to_cast("async function main() { console.log(5); }\n", "js")
        .expect("walk");
    let e = format!(
        "{:#}",
        crush_frontend::compiler::Compiler::new()
            .compile(cast)
            .expect_err("refused")
    );
    assert!(
        e.contains("an async `main` cannot be the entry point"),
        "{e}"
    );
    assert!(!e.contains("Write `fn main`"), "{e}");
}

/// An imported module's own `async fn main` is not the program's entry point
/// (the import resolver drops a module's `main`), so it keeps working.
#[test]
fn an_imported_module_with_async_fn_main_still_runs() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("geom.crush"),
        "fn area(w, h) { return w * h }\nasync fn main() { print(99) }\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("main.crush"),
        "import geom\nfn main() { print(geom.area(3, 2)) }\n",
    )
    .unwrap();
    let bin = option_env!("CARGO_BIN_EXE_crush-run").unwrap_or("crush-run");
    let out = Command::new(bin)
        .args(["run", "main.crush"])
        .current_dir(dir.path())
        .env_remove("CRUSH_PATH")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run crush-run");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stderr: {stderr}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "6\n",
        "stderr: {stderr}"
    );
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
