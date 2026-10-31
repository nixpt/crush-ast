//! `read` lowered over the `io.read` capability.
//!
//! The shared oracle corpus deliberately leaves `read` out: it runs each script
//! in-process, and a script that reads stdin would block on (or race) the test
//! harness's own stdin. These tests drive the portable VM with *supplied*
//! input instead, so they are deterministic and never touch stdin.
//!
//! Documented limitation: `io.read` reports end of input as an empty string, so
//! a blank line cannot be told apart from EOF. `read` treats an empty line as
//! end of input (status 1, variables empty), which keeps `while read` loops
//! terminating instead of spinning.

use crush_vm::{InputSource, PortableVm, Quotas};

fn run_with_input(src: &str, input: &str) -> (String, i32) {
    let cast = crush_lang_bash::bash_to_cast(src).expect("bash → CAST");
    let casm = crush_frontend::compiler::Compiler::new().compile(cast).expect("CAST → CASM");
    let prog = crush_lang_sdk::compile::casm_to_vm(&casm).expect("CASM → CVM1");
    let mut vm = PortableVm::new(prog);
    vm.set_quotas(Quotas { max_steps: 2_000_000, ..Default::default() });
    vm.set_input(InputSource::supplied(input));
    match vm.run() {
        Ok(_) => (vm.take_output(), 0),
        Err(crush_vm::VmError::Exit(code)) => (vm.take_output(), code),
        Err(e) => panic!("runtime error: {e}"),
    }
}

#[test]
fn read_one_name() {
    let (out, code) = run_with_input("read x\necho \"[$x]\"\n", "hello world\n");
    assert_eq!(out, "[hello world]\n");
    assert_eq!(code, 0);
}

#[test]
fn read_strips_surrounding_whitespace() {
    let (out, _) = run_with_input("read x\necho \"[$x]\"\n", "  spaced  \n");
    assert_eq!(out, "[spaced]\n");
}

#[test]
fn read_splits_into_names_and_keeps_the_rest() {
    let (out, _) = run_with_input("read a b\necho \"[$a][$b]\"\n", "one two   three\n");
    assert_eq!(out, "[one][two   three]\n");
}

#[test]
fn read_three_names() {
    let (out, _) = run_with_input("read a b c\necho \"[$a][$b][$c]\"\n", "1 2 3 4\n");
    assert_eq!(out, "[1][2][3 4]\n");
}

#[test]
fn read_missing_fields_are_empty() {
    let (out, _) = run_with_input("read a b c\necho \"[$a][$b][$c]\"\n", "solo\n");
    assert_eq!(out, "[solo][][]\n");
}

#[test]
fn read_defaults_to_reply() {
    let (out, _) = run_with_input("read\necho \"[$REPLY]\"\n", "typed\n");
    assert_eq!(out, "[typed]\n");
}

#[test]
fn read_r_keeps_backslashes() {
    let (out, _) = run_with_input("read -r x\necho \"[$x]\"\n", "a\\tb\n");
    assert_eq!(out, "[a\\tb]\n");
}

#[test]
fn read_without_r_removes_backslashes() {
    let (out, _) = run_with_input("read x\necho \"[$x]\"\n", "a\\tb\n");
    assert_eq!(out, "[atb]\n");
}

#[test]
fn read_without_r_continues_a_trailing_backslash() {
    let (out, _) = run_with_input("read x\necho \"[$x]\"\n", "one\\\ntwo\n");
    assert_eq!(out, "[onetwo]\n");
}

#[test]
fn read_with_prompt() {
    let (out, _) = run_with_input("read -p 'name? ' x\necho \"[$x]\"\n", "bob\n");
    assert_eq!(out, "name? [bob]\n");
}

#[test]
fn read_reports_eof_with_status_one() {
    let (out, code) = run_with_input("read x\necho \"[$x] $?\"\n", "");
    assert_eq!(out, "[] 1\n");
    assert_eq!(code, 0);
}

#[test]
fn read_loop_stops_at_end_of_input() {
    // The documented blank-line limitation: an empty line ends the loop.
    let (out, _) = run_with_input(
        "n=0\nwhile read -r line; do echo \"got $line\"; n=$((n+1)); done\necho \"count $n\"\n",
        "a\nb\n",
    );
    assert_eq!(out, "got a\ngot b\ncount 2\n");
}

#[test]
fn read_alias_and_append() {
    let (out, _) = run_with_input("read x\nread y\necho \"$x$y\"\n", "one\ntwo\n");
    assert_eq!(out, "onetwo\n");
}
