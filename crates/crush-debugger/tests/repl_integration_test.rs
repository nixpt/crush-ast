//! Integration test for the `crush-debugger run` REPL loop.
//!
//! Starts the debugger binary with a minimal `.crush` fixture, pipes
//! REPL commands via stdin, and asserts expected output.

use std::io::Write;
use std::process::{Command, Stdio};

fn spawn_debugger(args: &[&str], stdin_bytes: &[u8]) -> (String, String, bool) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_crush-debugger"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn crush-debugger");

    let mut stdin = child.stdin.take().expect("stdin not available");
    stdin
        .write_all(stdin_bytes)
        .expect("failed to write to stdin");
    drop(stdin);

    let output = child
        .wait_with_output()
        .expect("failed to wait on crush-debugger");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (stdout, stderr, output.status.success())
}

/// Start the debugger, send `help` + `quit`, verify output.
#[test]
fn repl_help_shows_commands_banner_and_quit_prints_bye() {
    let (stdout, stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/hello.crush", "--cap", "io.print"],
        b"help\nquit\n",
    );

    assert!(stdout.contains("Commands:"), "help output should contain 'Commands:' banner\nstdout:\n{}\nstderr:\n{}", stdout, stderr);
    assert!(stdout.contains("step | s"), "help output should list step command\nstdout:\n{}\nstderr:\n{}", stdout, stderr);
    assert!(stdout.contains("bye."), "quit should print 'bye.'\nstdout:\n{}\nstderr:\n{}", stdout, stderr);
    assert!(success, "should exit 0");
    assert!(stderr.is_empty(), "stderr should be empty, got:\n{}", stderr);
}

/// Verify that step reports instruction count and yielded status.
#[test]
fn repl_step_increments_and_reports() {
    let (stdout, _stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/hello.crush", "--cap", "io.print"],
        b"step\nstep\nquit\n",
    );

    assert!(stdout.contains("step 1: yielded=false"), "first step should report count 1\nstdout:\n{}", stdout);
    assert!(stdout.contains("step 2: yielded=false"), "second step should report count 2\nstdout:\n{}", stdout);
    assert!(success);
}

/// Verify that `--break <FILE>:<LINE>` sets breakpoints before the REPL
/// starts, visible immediately via `list`.
#[test]
fn cli_break_flag_sets_breakpoints_visible_in_repl() {
    let (stdout, stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/hello.crush", "--cap", "io.print",
          "--break", "hello.crush:1", "--break", "hello.crush:3"],
        b"list\nquit\n",
    );

    assert!(stdout.contains("#0: hello.crush:1"), "list should show breakpoint #0\nstdout:\n{}\nstderr:\n{}", stdout, stderr);
    assert!(stdout.contains("#1: hello.crush:3"), "list should show breakpoint #1\nstdout:\n{}\nstderr:\n{}", stdout, stderr);
    assert!(stderr.contains("breakpoint #0 set at hello.crush:1"), "stderr should confirm breakpoint #0\nstderr:\n{}", stderr);
    assert!(stderr.contains("breakpoint #1 set at hello.crush:3"), "stderr should confirm breakpoint #1\nstderr:\n{}", stderr);
    assert!(success);
}

/// Verify that list with no breakpoints reports empty.
#[test]
fn repl_list_reports_no_breakpoints_when_empty() {
    let (stdout, _stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/hello.crush", "--cap", "io.print"],
        b"list\nquit\n",
    );

    assert!(stdout.contains("no breakpoints"), "list should report 'no breakpoints'\nstdout:\n{}", stdout);
    assert!(success);
}

/// Verify that `continue` runs the VM to completion and reports "done".
#[test]
fn repl_continue_reports_done() {
    let (stdout, _stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/hello.crush", "--cap", "io.print"],
        b"continue\nquit\n",
    );

    assert!(stdout.contains("done"), "continue should report 'done'\nstdout:\n{}", stdout);
    assert!(success);
}

/// Verify that `--max-steps` sets the VM's step quota and `continue`
/// respects it, reporting "quota exceeded" when hit.
#[test]
fn max_steps_flag_respects_quota_on_continue() {
    let (stdout, stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/hello.crush", "--cap", "io.print", "--max-steps", "2"],
        b"continue\nquit\n",
    );

    assert!(stdout.contains("quota exceeded (2)"), "should report 'quota exceeded (2)'\nstdout:\n{}\nstderr:\n{}", stdout, stderr);
    assert!(stdout.contains("bye."), "REPL should still be alive after quota exceeded\nstdout:\n{}\nstderr:\n{}", stdout, stderr);
    assert!(success);
}

/// Verify that `step` also respects `--max-steps`: after stepping past
/// the quota, the next `step` fails with a clear instruction-quota error.
#[test]
fn max_steps_flag_respects_quota_on_step() {
    // hello.crush has 3 instructions. With --max-steps 1:
    //   step 1: PUSH_STR "hello" (steps 0→1, 0 < 1 OK)
    //   step 2: check quota: 1 >= 1 → StepQuota → error
    let (stdout, stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/hello.crush", "--cap", "io.print", "--max-steps", "1"],
        b"step\nstep\nquit\n",
    );

    assert!(
        stdout.contains("step 1: yielded=false"),
        "first step should succeed\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("quota exceeded (1)"),
        "second step should report quota exceeded (1)\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("bye."),
        "REPL should stay alive after quota error\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(success);
}

/// Verify that running without required capabilities produces a clear
/// error ("capability not declared in manifest") rather than a vague VM
/// failure.
#[test]
fn missing_capability_reports_clear_error() {
    let (stdout, stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/hello.crush"], // NO --cap
        b"continue\nquit\n",
    );

    assert!(stdout.contains("error:"), "should print an error line\nstdout:\n{}\nstderr:\n{}", stdout, stderr);
    assert!(stdout.contains("capability not declared"), "error should mention 'capability not declared'\nstdout:\n{}\nstderr:\n{}", stdout, stderr);
    assert!(stdout.contains("io.print"), "error should name the missing capability 'io.print'\nstdout:\n{}\nstderr:\n{}", stdout, stderr);
    assert!(success, "should exit 0");
}

/// Verify that `break` and `delete` work end to end.
#[test]
fn repl_set_and_delete_breakpoint() {
    let (stdout, _stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/hello.crush", "--cap", "io.print"],
        b"break hello.crush:1\ndelete 0\nlist\nquit\n",
    );

    assert!(stdout.contains("breakpoint #0 set at hello.crush:1"), "should report breakpoint set\nstdout:\n{}", stdout);
    assert!(stdout.contains("breakpoint #0 removed"), "should report breakpoint removed\nstdout:\n{}", stdout);
    assert!(stdout.contains("no breakpoints"), "list after delete should report 'no breakpoints'\nstdout:\n{}", stdout);
    assert!(success);
}

/// Verify that a CLI `--break` flag resolves via the sourcemap and
/// actually triggers a VM pause on `continue`.
#[test]
fn cli_break_triggers_vm_hit_on_continue() {
    // hello.crush has 3 instructions:
    //   Line 1: PUSH_STR "hello"
    //   Line 2: CAP_CALL "io.print" 1
    //   Line 3: HALT
    // With --break hello.crush:2, the sourcemap resolves line 2 →
    // bytecode offset, and continue should hit it after step 1.
    let (stdout, stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/hello.crush", "--cap", "io.print",
          "--break", "hello.crush:2"],
        b"continue\nquit\n",
    );

    assert!(
        stdout.contains("hit breakpoint #0"),
        "continue should hit the breakpoint set at line 2\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("bye."),
        "REPL should be alive after breakpoint hit\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(success);
}

/// Verify that a REPL `break` command also resolves via the sourcemap
/// and triggers a VM hit.
#[test]
fn repl_break_triggers_vm_hit_on_continue() {
    // Set breakpoint at line 2 via REPL command, then continue.
    let (stdout, stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/hello.crush", "--cap", "io.print"],
        b"break hello.crush:2\ncontinue\nquit\n",
    );

    assert!(
        stdout.contains("breakpoint #0 set at hello.crush:2"),
        "should confirm breakpoint set\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("hit breakpoint #0"),
        "continue should hit the breakpoint set via REPL\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(success);
}

/// Verify that `--max-stack` sets the stack depth quota and `continue`
/// reports a clean "quota exceeded" message when hit.
#[test]
fn max_stack_flag_hits_stack_quota_on_continue() {
    // hello.crush pushes "hello" then calls CAP_CALL.
    // With --max-stack 0, the second step (before the CAP_CALL) will hit
    // the stack quota because the stack has 1 entry and 1 > 0.
    let (stdout, stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/hello.crush", "--cap", "io.print", "--max-stack", "0"],
        b"continue\nquit\n",
    );

    assert!(
        stdout.contains("quota exceeded (0)"),
        "should report quota exceeded (0)\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("bye."),
        "REPL should stay alive after error\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(success);
}

/// Verify that `--max-output` sets the output byte quota and `continue`
/// reports a clean "quota exceeded" message when hit.
#[test]
fn max_output_flag_hits_output_quota_on_continue() {
    // hello.crush prints "hello" (5 bytes). With --max-output 3,
    // the io.print call should fail with OutputQuota.
    let (stdout, stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/hello.crush", "--cap", "io.print", "--max-output", "3"],
        b"continue\nquit\n",
    );

    assert!(
        stdout.contains("quota exceeded (3)"),
        "should report quota exceeded (3)\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("bye."),
        "REPL should stay alive after error\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(success);
}

/// Verify that `--max-call-depth` sets the call depth quota and `continue`
/// reports a clean "quota exceeded" message when a recursive function
/// exceeds it.
#[test]
fn max_call_depth_flag_hits_call_depth_quota_on_continue() {
    // call_depth.crush has a recursive function that calls itself.
    // With --max-call-depth 1, the second frame push will fail.
    let (stdout, stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/call_depth.crush", "--max-call-depth", "1"],
        b"continue\nquit\n",
    );

    assert!(
        stdout.contains("quota exceeded (1)"),
        "should report quota exceeded (1)\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("bye."),
        "REPL should stay alive after error\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(success);
}

/// Verify that `--break` and `--max-steps` compose correctly: the
/// breakpoint fires first (before the quota), and the second
/// `continue` hits the step quota.
#[test]
fn breakpoint_fires_before_step_quota() {
    // hello.crush: PUSH_STR (line 1), CAP_CALL (line 2), HALT (line 3).
    // --break line 2 --max-steps 2:
    //   continue #1: step 1 executes PUSH_STR, hits breakpoint at line 2
    //                → "hit breakpoint #0", steps=1
    //   continue #2: resumes through CAP_CALL (step 2), next step check
    //                sees 2 >= 2 → "quota exceeded (2)"
    let (stdout, stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/hello.crush", "--cap", "io.print",
          "--break", "hello.crush:2", "--max-steps", "2"],
        b"continue\ncontinue\nquit\n",
    );

    assert!(
        stdout.contains("hit breakpoint #0"),
        "first continue should hit breakpoint\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("quota exceeded (2)"),
        "second continue should hit step quota\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("bye."),
        "REPL should stay alive after\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(success);
}

/// Verify that `--break` and `--max-stack` compose correctly: the
/// breakpoint fires first (before the stack quota is checked), and
/// the second `continue` hits the stack quota.
#[test]
fn breakpoint_fires_before_stack_quota() {
    // hello.crush: PUSH_STR (line 1), CAP_CALL (line 2), HALT (line 3).
    // --break line 2 --max-stack 0:
    //   continue #1: step 1 executes PUSH_STR, hits breakpoint at line 2
    //                → "hit breakpoint #0", stack=["hello"]
    //   continue #2: resumes, check_stack_quota sees 1 > 0
    //                → "quota exceeded (0)"
    let (stdout, stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/hello.crush", "--cap", "io.print",
          "--break", "hello.crush:2", "--max-stack", "0"],
        b"continue\ncontinue\nquit\n",
    );

    assert!(
        stdout.contains("hit breakpoint #0"),
        "first continue should hit breakpoint\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("quota exceeded (0)"),
        "second continue should hit stack quota\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("bye."),
        "REPL should stay alive after\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(success);
}

/// Verify that a CLI `--break` and REPL `break` at the same location
/// produce distinct IDs visible in `list`, and BOTH fire on successive
/// `continue` calls before the instruction executes.
#[test]
fn cli_and_repl_break_at_same_location_produce_distinct_ids_and_both_fire() {
    // --break hello.crush:2 sets breakpoint #0 at PUSH_STR via CLI.
    // Then REPL `break hello.crush:2` sets #1 at the same line.
    // Both appear in `list` with distinct IDs.
    //   continue #1 → hit breakpoint #0 (VM fires first BP at this IP)
    //   continue #2 → hit breakpoint #1 (VM fires second BP at this IP)
    //   continue #3 → done (instruction finally executes, runs to HALT)
    let (stdout, stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/hello.crush", "--cap", "io.print",
          "--break", "hello.crush:2"],
        b"break hello.crush:2\nlist\ncontinue\ncontinue\ncontinue\nquit\n",
    );

    assert!(
        stdout.contains("breakpoint #1 set at hello.crush:2"),
        "REPL break should assign id #1 at same location\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("#0: hello.crush:2"),
        "list should show breakpoint #0\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("#1: hello.crush:2"),
        "list should show breakpoint #1 at same location\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("hit breakpoint #0"),
        "first continue should hit breakpoint #0\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("hit breakpoint #1"),
        "second continue should hit breakpoint #1 (per-IP skip tracking)\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("done"),
        "third continue should run to completion\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("bye."),
        "REPL should stay alive after\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(success);
}

/// Verify that `--break` and `--max-output` compose correctly: the
/// breakpoint fires first (before the CAP_CALL that produces output),
/// and the second `continue` executes CAP_CALL and hits the output quota.
#[test]
fn breakpoint_fires_before_output_quota() {
    // hello.crush: PUSH_STR (line 1), CAP_CALL (line 2), HALT (line 3).
    // --break line 2 --max-output 3:
    //   continue #1: step 1 executes PUSH_STR, hits breakpoint at line 2
    //                → "hit breakpoint #0"
    //   continue #2: resumes through CAP_CALL, io.print adds 5 bytes,
    //                5 > 3 → "quota exceeded (3)"
    let (stdout, stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/hello.crush", "--cap", "io.print",
          "--break", "hello.crush:2", "--max-output", "3"],
        b"continue\ncontinue\nquit\n",
    );

    assert!(
        stdout.contains("hit breakpoint #0"),
        "first continue should hit breakpoint\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("quota exceeded (3)"),
        "second continue should hit output quota\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("bye."),
        "REPL should stay alive after\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(success);
}

/// Verify that `step` also respects `--max-stack`: stepping past the
/// stack quota produces a clean "quota exceeded" message.
#[test]
fn max_stack_flag_hits_stack_quota_on_step() {
    // hello.crush: PUSH_STR "hello" then CAP_CALL.
    // --max-stack 0: step 1 pushes "hello" (stack len 0→1, OK at check time).
    // Step 2: check_stack_quota sees 1 > 0 → StackQuota → "quota exceeded (0)".
    let (stdout, stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/hello.crush", "--cap", "io.print", "--max-stack", "0"],
        b"step\nstep\nquit\n",
    );

    assert!(
        stdout.contains("step 1: yielded=false"),
        "first step should succeed\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("quota exceeded (0)"),
        "second step should report quota exceeded (0)\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("bye."),
        "REPL should stay alive after error\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(success);
}

/// Verify that `step` also respects `--max-output`: stepping through
/// io.print with a tight output quota produces a clean "quota exceeded"
/// message.
#[test]
fn max_output_flag_hits_output_quota_on_step() {
    // hello.crush prints "hello" (5 bytes). --max-output 3.
    // Step 1: PUSH_STR "hello" — succeeds.
    // Step 2: CAP_CALL "io.print" → 5 > 3 → OutputQuota → "quota exceeded (3)".
    let (stdout, stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/hello.crush", "--cap", "io.print", "--max-output", "3"],
        b"step\nstep\nquit\n",
    );

    assert!(
        stdout.contains("step 1: yielded=false"),
        "first step should succeed\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("quota exceeded (3)"),
        "second step should report quota exceeded (3)\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("bye."),
        "REPL should stay alive after error\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(success);
}

/// Verify that `--break` on a comment/directive line (not in the
/// sourcemap) emits a clear stderr warning and the VM still runs to
/// completion (the breakpoint has no bytecode address, so it never
/// pauses).
#[test]
fn break_on_directive_line_warns_and_runs_to_completion() {
    // hello.crush line 1 is `.func main` — a directive, not in
    // the assembler sourcemap. The CLI `--break hello.crush:1`
    // should warn that the line can't be resolved but still allow
    // `continue` to run through to HALT.
    let (stdout, stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/hello.crush", "--cap", "io.print",
          "--break", "hello.crush:1"],
        b"continue\nquit\n",
    );

    assert!(
        stderr.contains("warning: breakpoint at hello.crush:1"),
        "stderr should warn about unresolvable breakpoint line\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stderr.contains("line not in sourcemap"),
        "stderr should explain why the line can't be resolved\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("done"),
        "continue should run to completion (breakpoint has no bytecode address)\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("bye."),
        "REPL should stay alive after\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(success);
}

/// Verify that `status` reports instruction count and no pause point
/// when the VM hasn't hit any breakpoints yet.
#[test]
fn repl_status_reports_instructions_and_no_pause() {
    let (stdout, _stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/hello.crush", "--cap", "io.print"],
        b"status\nquit\n",
    );

    assert!(
        stdout.contains("instructions: 0"),
        "status should show instruction count 0 before any step\nstdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("paused at: (none)"),
        "status should show no pause point before breakpoints are hit\nstdout:\n{}",
        stdout
    );
    assert!(stdout.contains("bye."), "REPL should stay alive");
    assert!(success);
}

/// Verify that `status` after hitting a breakpoint reports the active
/// breakpoint location as the pause point.
#[test]
fn repl_status_after_breakpoint_shows_paused_at() {
    let (stdout, stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/hello.crush", "--cap", "io.print",
          "--break", "hello.crush:2"],
        b"continue\nstatus\nquit\n",
    );

    assert!(
        stdout.contains("hit breakpoint #0"),
        "continue should hit the breakpoint first\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("instructions: 1"),
        "status should report 1 instruction executed\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("paused at: hello.crush:2"),
        "status should report the breakpoint location as paused-at\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(stdout.contains("bye."));
    assert!(success);
}

/// Verify that `step` also respects `--max-call-depth`: the first CALL
/// into a recursive function exceeds the quota and produces a clean
/// "quota exceeded" message.
#[test]
fn max_call_depth_flag_hits_call_depth_quota_on_step() {
    // call_depth.crush: main calls recurse. --max-call-depth 1.
    // Step 1: CALL recurse pushes second frame → 2 >= 1 → CallDepthQuota
    // → "quota exceeded (1)".
    let (stdout, stderr, success) = spawn_debugger(
        &["run", "tests/fixtures/call_depth.crush", "--max-call-depth", "1"],
        b"step\nquit\n",
    );

    assert!(
        stdout.contains("quota exceeded (1)"),
        "first step should report quota exceeded (1)\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("bye."),
        "REPL should stay alive after error\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(success);
}
