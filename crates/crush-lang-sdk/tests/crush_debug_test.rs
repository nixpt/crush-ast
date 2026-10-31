//! CRUSH-317: `crush-debug` / `crush debug` against today's language: a
//! closure capturing a variable, a struct with an `impl` method, a loop and a
//! function call (`tests/fixtures/debug_demo.crush`).
//!
//! Every run feeds commands on a pipe and closes it: the debugger reads
//! stdin, so it must never be left open.

use std::io::Write;
use std::process::{Command, Stdio};

const DEMO: &str = "tests/fixtures/debug_demo.crush";

struct Run {
    stdout: String,
    stderr: String,
    ok: bool,
}

fn run(bin: &str, args: &[&str], stdin: &str) -> Run {
    let mut child = Command::new(bin)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    Run {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        ok: out.status.success(),
    }
}

fn debug(args: &[&str], stdin: &str) -> Run {
    run(env!("CARGO_BIN_EXE_crush-debug"), args, stdin)
}

#[test]
fn breaks_inside_a_closure_and_a_method_and_reads_captured_and_field_values() {
    // 18: `let scaled = n * k` in the closure; 11: `let sum = ...` in the method.
    let r = debug(
        &[
            DEMO,
            "--step",
            "--inspect",
            "--break",
            "18",
            "--break",
            "11",
        ],
        "continue\nprint k\nprint n\nlocals\ncontinue\ncontinue\nprint self.x\nprint self.y\nwatch sum\nnext\nnext\ncontinue\n",
    );
    assert!(r.ok, "{}\n{}", r.stdout, r.stderr);
    let out = &r.stdout;
    // The closure: stops on its line, shows the source, reads the captured `k`.
    assert!(
        out.contains("hit breakpoint #0 at tests/fixtures/debug_demo.crush:18 in __lambda_0_scale"),
        "{out}"
    );
    assert!(out.contains("18 |         let scaled = n * k"), "{out}");
    assert!(out.contains("k = 3"), "{out}");
    assert!(out.contains("n = 1"), "{out}");
    // The method: a struct field.
    assert!(out.contains("in Point::total"), "{out}");
    assert!(out.contains("self.x = 4"), "{out}");
    assert!(out.contains("self.y = 5"), "{out}");
    // A watch on `sum` fires when the next line assigns it; `next` steps by line.
    assert!(out.contains("(unset) -> 9"), "{out}");
    // The program's own output arrives, and it runs to the end.
    assert!(out.contains("9\n18"), "{out}");
    assert!(out.contains("done"), "{out}");
}

#[test]
fn finish_returns_to_the_caller() {
    let r = debug(
        &[DEMO, "--step", "--inspect", "--break", "4"],
        "continue\nfinish\n",
    );
    assert!(r.ok, "{}\n{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("in apply"), "{}", r.stdout);
    // `apply` (line 4) was entered from main's loop; finishing lands back in a caller.
    let after_finish = r.stdout.rsplit("cru-s-debugger> ").nth(1).unwrap();
    assert!(after_finish.contains("stopped at"), "{after_finish}");
}

#[test]
fn without_flags_nothing_runs_and_the_refusal_names_the_flag() {
    let r = debug(&[DEMO], "break 18\ncontinue\nprint k\nlist\nstatus\n");
    assert!(r.ok, "{}\n{}", r.stdout, r.stderr);
    assert!(
        r.stdout
            .contains("`break` needs the `debug.step` grant (pass --step)"),
        "{}",
        r.stdout
    );
    assert!(
        r.stdout
            .contains("`continue` needs the `debug.step` grant (pass --step)"),
        "{}",
        r.stdout
    );
    assert!(r.stdout.contains("`print` needs the `debug.inspect.redacted` grant (pass --step --inspect=redacted, or --inspect)"), "{}", r.stdout);
    // The program did not run: no output, no stop.
    assert!(!r.stdout.contains("hit breakpoint"), "{}", r.stdout);
    assert!(!r.stdout.contains("\n9\n"), "{}", r.stdout);
}

#[test]
fn step_alone_hides_values_and_redacted_shows_only_type_and_hash() {
    // `--step` controls the program but may not look at it: `print` is refused...
    let hidden = debug(&[DEMO, "--step", "--break", "18"], "continue\nprint k\n");
    assert!(
        hidden
            .stdout
            .contains("`print` needs the `debug.inspect.redacted` grant"),
        "{}",
        hidden.stdout
    );
    // ...and a stop's frames, as an editor sees them, show every value as hidden.
    let json = debug(
        &[DEMO, "--step", "--break", "18", "--message-format", "json"],
        "continue\n",
    );
    assert!(json.stdout.contains("{\"hidden\":true}"), "{}", json.stdout);
    assert!(!json.stdout.contains("\"text\":\"3\""), "{}", json.stdout);

    let redacted = debug(
        &[DEMO, "--step", "--inspect=redacted", "--break", "18"],
        "continue\nprint k\n",
    );
    assert!(
        redacted.stdout.contains("k = <int #"),
        "{}",
        redacted.stdout
    );
    assert!(!redacted.stdout.contains("k = 3"), "{}", redacted.stdout);
}

#[test]
fn inspect_without_step_and_break_without_step_are_errors() {
    for args in [
        [DEMO, "--inspect"].as_slice(),
        [DEMO, "--break", "18"].as_slice(),
    ] {
        let r = debug(args, "");
        assert!(!r.ok, "{args:?}");
        assert!(r.stderr.contains("--step"), "{}", r.stderr);
    }
}

#[test]
fn a_program_that_needs_a_capability_it_is_not_granted_is_refused_up_front() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("env.crush");
    std::fs::write(
        &path,
        "fn main() {\n    print(env.get(\"HOME\"))\n}\nmain()\n",
    )
    .unwrap();
    let r = debug(&[path.to_str().unwrap(), "--step"], "continue\n");
    assert!(!r.ok);
    assert!(r.stderr.contains("does not grant"), "{}", r.stderr);
    assert!(r.stderr.contains("--env"), "{}", r.stderr);
}

#[test]
fn json_mode_is_one_event_per_line() {
    let r = debug(
        &[
            DEMO,
            "--step",
            "--inspect",
            "--message-format",
            "json",
            "--break",
            "18",
        ],
        "continue\nprint k\nbogus\nwatch scaled\ncontinue\ndelete 0\nunwatch 0\ncontinue\nquit\n",
    );
    assert!(r.ok, "{}\n{}", r.stdout, r.stderr);
    assert!(!r.stdout.contains("cru-s-debugger>"), "{}", r.stdout);
    let lines: Vec<serde_json::Value> = r
        .stdout
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON: {l}: {e}")))
        .collect();
    let event = |name: &str| lines.iter().find(|l| l["event"] == name);

    assert_eq!(event("started").unwrap()["visibility"], "Full");
    let stop = lines.iter().find(|l| l["event"] == "stopped").unwrap();
    assert_eq!(stop["reason"]["kind"], "breakpoint");
    assert_eq!(stop["line"], 18);
    assert_eq!(stop["function"], "__lambda_0_scale");
    assert_eq!(stop["source"], "        let scaled = n * k");
    let top = &stop["frames"][0];
    assert!(
        top["locals"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["name"] == "k" && l["value"]["text"] == "3")
    );
    // `print` is a message; an unknown command is a crush-diagnostics record.
    assert!(
        lines
            .iter()
            .any(|l| l["event"] == "message" && l["text"] == "k = 3")
    );
    let diag = lines.iter().find(|l| l["code"] == "E-DBG01").unwrap();
    assert_eq!(diag["level"], "error");
    // A watchpoint stop names the variable; the program's output and end are events.
    assert!(
        lines
            .iter()
            .any(|l| l["reason"]["kind"] == "watchpoint" && l["reason"]["name"] == "scaled")
    );
    assert!(lines.iter().any(|l| l["event"] == "output"));
    assert!(event("finished").is_some());
}

#[test]
fn json_mode_refusal_carries_the_flag() {
    let r = debug(&[DEMO, "--message-format", "json"], "continue\n");
    let refused: serde_json::Value = r
        .stdout
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .find(|l| l["event"] == "refused")
        .expect("a refused event");
    assert_eq!(refused["needs"], "debug.step");
    assert_eq!(refused["hint"], "pass --step");
}

#[test]
fn json_mode_never_leaks_values_the_grants_do_not_allow() {
    let r = debug(
        &[
            DEMO,
            "--step",
            "--inspect=redacted",
            "--message-format",
            "json",
            "--break",
            "11",
        ],
        "continue\nprint self.x\nlocals\n",
    );
    assert!(r.ok, "{}\n{}", r.stdout, r.stderr);
    assert!(!r.stdout.contains("\"text\":\"4\""), "{}", r.stdout);
    assert!(!r.stdout.contains("{x: 4"), "{}", r.stdout);
    assert!(r.stdout.contains("\"hash\""), "{}", r.stdout);
}

#[test]
fn crush_debug_subcommand_delegates_and_is_listed_in_help() {
    let crush = env!("CARGO_BIN_EXE_crush");
    let help = run(crush, &["--help"], "");
    assert!(help.stderr.contains("crush debug FILE"), "{}", help.stderr);
    // `crush debug` execs the sibling `crush-debug`, which cargo puts in the same directory.
    let r = run(
        crush,
        &["debug", DEMO, "--step", "--inspect", "--break", "11"],
        "continue\nprint self.x\n",
    );
    assert!(r.ok, "{}\n{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("self.x = 4"), "{}", r.stdout);
}

#[test]
fn casm_files_still_debug_at_instruction_level() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.casm");
    std::fs::write(&path, ".func main\n    PUSH 1\n    STORE 0\n    HALT\n").unwrap();
    let r = debug(
        &[path.to_str().unwrap(), "--step", "--inspect"],
        "step\nprint 0\nstep\nprint 0\n",
    );
    assert!(r.ok, "{}\n{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("slot 0 = 1"), "{}", r.stdout);
}

#[test]
fn a_breakpoint_on_a_blank_line_says_where_the_code_is() {
    let r = debug(&[DEMO, "--step", "--break", "2"], "list\n");
    assert!(r.ok, "{}\n{}", r.stdout, r.stderr);
    assert!(
        r.stdout.contains("no code on line 2, so it will not fire"),
        "{}",
        r.stdout
    );
    assert!(
        r.stdout.contains("the next line with code is"),
        "{}",
        r.stdout
    );
}
