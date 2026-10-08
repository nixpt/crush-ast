//! #94 / CRUSH-176: PortableVm must run real programs exactly like the
//! scheduler (`crush_vm::run`). The debugger, crush-web's `Session` /
//! `execute_with` and exo-light's capsule runner all step PortableVm.
//!
//! Corpus: four games from awesome-crush (github.com/nixpt/awesome-crush,
//! `games/`), kept in `examples/crush/`. Each one failed on PortableVm with
//! `stack underflow` or `truncated instruction` while the scheduler finished.

use std::io::Write;
use std::process::{Command, Stdio};

use crush_vm::{InputSource, PortableVm, Quotas};

fn example(name: &str) -> String {
    let path = format!(
        "{}/../../examples/crush/{name}.crush",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// (finished ok?, output, error) from the scheduler.
fn on_scheduler(program: &crush_vm::Program) -> (bool, String, String) {
    match crush_vm::run(program, &Quotas::default()) {
        Ok(r) => (true, r.output, String::new()),
        Err(e) => (false, String::new(), e.to_string()),
    }
}

/// Step PortableVm to completion the way crush-web's `Session` does.
fn on_portable(program: &crush_vm::Program, input: InputSource) -> (bool, String, String) {
    let mut vm = PortableVm::new(program.clone());
    vm.set_input(input);
    loop {
        if vm.is_halted() {
            return (true, vm.take_output(), String::new());
        }
        match vm.step() {
            Ok(None) => {}
            Ok(Some(y)) => return (false, vm.take_output(), format!("unexpected pause: {y:?}")),
            Err(e) => return (false, vm.take_output(), e.to_string()),
        }
    }
}

fn assert_parity(name: &str) {
    let program = crush_lang_sdk::compile::compile_crush_source(&example(name))
        .unwrap_or_else(|e| panic!("{name}: compile: {e}"));
    let sched = on_scheduler(&program);
    let port = on_portable(&program, InputSource::supplied(""));
    assert!(sched.0, "{name}: scheduler failed: {}", sched.2);
    assert!(!sched.1.is_empty(), "{name}: scheduler printed nothing");
    assert_eq!(
        port, sched,
        "{name}: PortableVm diverges from the scheduler"
    );
}

#[test]
fn tictactoe_matches_scheduler() {
    assert_parity("tictactoe");
}

#[test]
fn lights_out_matches_scheduler() {
    assert_parity("lights_out");
}

#[test]
fn blackjack_matches_scheduler() {
    assert_parity("blackjack");
}

/// The scheduler only reads `io.read` from process stdin, so the reference
/// run for the interactive game is the `crush-run` binary with piped input.
fn crush_run_with_stdin(src: &str, stdin: &str) -> (bool, String, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("game.crush");
    std::fs::write(&path, src).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_crush-run"))
        .args(["run", "--cap", "io.read", path.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn crush-run");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// The issue comment's table: every multi-round game printed all of its
/// output and then failed while unwinding the recursion.
#[test]
fn blackjack_interactive_multi_round_matches_crush_run() {
    let src = example("blackjack_interactive");
    let program = crush_lang_sdk::compile::compile_crush_source(&src).unwrap();
    for inputs in [
        "10\nh\ns\n0\n",
        "25\ns\n10\nh\ns\n0\n",
        "10\ns\n10\ns\n0\n",
        "10\nh\ns\n10\nh\ns\n10\ns\n0\n",
    ] {
        let native = crush_run_with_stdin(&src, inputs);
        assert!(native.0, "crush-run failed on {inputs:?}: {}", native.2);
        let port = on_portable(&program, InputSource::supplied(inputs));
        assert_eq!(
            (port.0, port.1.as_str(), port.2.as_str()),
            (true, native.1.as_str(), ""),
            "inputs {inputs:?}"
        );
    }
}
