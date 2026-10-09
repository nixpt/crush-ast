//! AOT programs that churn through strings and arrays must run in bounded memory.
//!
//! Before the C backend had a collector (CRUSH-227) nothing was freed during a run:
//! 10,000 `a = a + [i]` appends kept every copy (~800 MB), a 100,000-char string built
//! one character at a time kept every prefix (~5 GB), and a loop creating 2^21
//! short-lived arrays stopped with `array pool exhausted`. Each case here runs under a
//! 256 MB address-space limit; the runner itself needs about 100 MB of that.

use std::path::Path;
use std::process::Command;

use crush_aot::AotCompiler;

const ADDRESS_SPACE_LIMIT: &str = "--as=268435456";

fn available(tool: &str) -> bool {
    Command::new(tool).arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

/// Run the built module under the address-space limit; returns its first stdout line.
fn run_limited(so: &Path) -> Result<String, String> {
    let out = Command::new("prlimit")
        .arg(ADDRESS_SPACE_LIMIT)
        .arg(env!("CARGO_BIN_EXE_crush-aot-runner"))
        .arg(so)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("exit {:?}: {}", out.status.code(), String::from_utf8_lossy(&out.stderr)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or("").to_string())
}

fn assert_bounded(name: &str, source: &str, expected: &str) {
    if !available("prlimit") {
        eprintln!("prlimit not found; skipping {name}");
        return;
    }
    let program = crush_frontend::compile_crush_source(source).expect("frontend");
    let compiler = AotCompiler::new();
    let mut builds = vec![("rust", compiler.compile_casm(&program, &format!("{name}_rust")).expect("rust backend"))];
    if available("gcc") {
        builds.push(("c", compiler.compile_c(&program, &format!("{name}_c"), "gcc").expect("c backend")));
    }
    for (backend, so) in builds {
        assert_eq!(run_limited(&so).as_deref(), Ok(expected), "{name} [{backend}]");
    }
}

#[test]
fn appending_to_an_array_in_a_loop() {
    assert_bounded(
        "mem_array_append",
        "fn main() { let a = []; let i = 0; while i < 10000 { a = a + [i]; i = i + 1; } io.print(len(a)); return 0; }",
        "10000",
    );
}

#[test]
fn building_a_long_string_one_char_at_a_time() {
    assert_bounded(
        "mem_string_build",
        r#"fn main() { let s = ""; let i = 0; while i < 100000 { s = s + "x"; i = i + 1; } io.print(len(s)); return 0; }"#,
        "100000",
    );
}

#[test]
fn millions_of_short_lived_arrays() {
    assert_bounded(
        "mem_short_lived",
        "fn pair(i) { return [i, i + 1]; } \
         fn main() { let t = 0; let i = 0; while i < 2100000 { let p = pair(i); t = t + p[1] - p[0]; i = i + 1; } io.print(t); return 0; }",
        "2100000",
    );
}
