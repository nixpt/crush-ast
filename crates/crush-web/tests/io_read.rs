//! Native tests for the stepped runners (`execute_with`, `Session`). The
//! program is `examples/crush/blackjack_interactive.crush` (from
//! awesome-crush's `games/`); the same file drives the headless-browser test
//! in `scripts/browser-test.sh`.

use crush_web::{RunOptions, Session, Status, execute_with_options};

const BLACKJACK: &str = include_str!("../../../examples/crush/blackjack_interactive.crush");

#[test]
fn execute_with_stdin_plays_a_round() {
    let options = RunOptions {
        stdin: Some("10\nh\ns\n0\n".into()),
        max_steps: None,
    };
    let result = execute_with_options(BLACKJACK, &options);
    assert!(result.ok, "{:?}", result.error);
    assert!(
        result.output.contains("Dealing (bet: $10)..."),
        "{}",
        result.output
    );
    assert!(result.output.contains("You stand at "), "{}", result.output);
    assert!(
        result.output.contains("You leave the table with $"),
        "{}",
        result.output
    );
}

#[test]
fn execute_with_no_stdin_is_eof() {
    let result = execute_with_options(BLACKJACK, &RunOptions::default());
    assert!(result.ok, "{:?}", result.error);
    assert!(
        result.output.contains("You leave the table with $100."),
        "{}",
        result.output
    );
}

#[test]
fn execute_with_keeps_output_printed_before_an_error() {
    let source = "fn main() {\n    io.print(\"before\");\n    let xs = [1];\n    io.print(xs[5] + 1);\n    return;\n}\n";
    let result = execute_with_options(source, &RunOptions::default());
    assert!(!result.ok);
    assert!(
        result
            .error
            .as_deref()
            .is_some_and(|e| e.starts_with("runtime error")),
        "{:?}",
        result.error
    );
    assert_eq!(result.output, "before\n");
}

#[test]
fn execute_with_step_budget_and_compile_error() {
    let options = RunOptions {
        stdin: None,
        max_steps: Some(50),
    };
    let result = execute_with_options(BLACKJACK, &options);
    assert!(!result.ok);
    assert!(
        result
            .error
            .as_deref()
            .is_some_and(|e| e.contains("instruction quota")),
        "{:?}",
        result.error
    );

    let result = execute_with_options("fn main( {", &RunOptions::default());
    assert!(!result.ok);
    assert!(
        result
            .error
            .as_deref()
            .is_some_and(|e| e.starts_with("compile error"))
    );
}

#[test]
fn session_plays_a_round_interactively() {
    let mut session = Session::with_options(BLACKJACK, &RunOptions::default());
    let r = session.run_report();
    assert_eq!(r.status, Status::NeedInput, "{:?}", r.error);
    assert!(r.output.contains("Bet (1-100)"), "{}", r.output);

    let r = session.provide_report("10");
    assert_eq!(r.status, Status::NeedInput);
    assert!(
        r.output.starts_with("\nDealing (bet: $10)..."),
        "{}",
        r.output
    );
    assert!(!r.output.contains("Bet (1-100)"), "output must be per-call");
    assert!(r.output.ends_with("[h]it or [s]tand?\n"), "{}", r.output);

    let r = session.provide_report("s");
    assert_eq!(r.status, Status::NeedInput);
    assert!(r.output.contains("You stand at "), "{}", r.output);
    assert!(r.output.contains("Bet (1-"), "{}", r.output);

    let r = session.provide_report("0");
    assert_eq!(r.status, Status::Done, "{:?}", r.error);
    assert!(
        r.output.contains("You leave the table with $"),
        "{}",
        r.output
    );

    // Matches a one-shot run fed the same lines.
    let options = RunOptions {
        stdin: Some("10\ns\n0\n".into()),
        max_steps: None,
    };
    assert_eq!(
        session.transcript(),
        execute_with_options(BLACKJACK, &options).output
    );

    // Finished sessions stay finished.
    let again = session.provide_report("10");
    assert_eq!((again.status, again.output.as_str()), (Status::Done, ""));
}

#[test]
fn session_close_is_eof_and_budget_spans_the_session() {
    let mut session = Session::with_options(BLACKJACK, &RunOptions::default());
    assert_eq!(session.run_report().status, Status::NeedInput);
    let r = session.close_report();
    assert_eq!(r.status, Status::Done);
    assert!(r.output.contains("You leave the table with $100."));

    // Steps to reach the second read with no budget, then a budget one short
    // of that: the quota trips during the round, after "Dealing" is printed.
    let mut unlimited = Session::with_options(BLACKJACK, &RunOptions::default());
    unlimited.run_report();
    let second_read = unlimited.provide_report("10").steps;
    let mut session = Session::with_options(
        BLACKJACK,
        &RunOptions {
            stdin: None,
            max_steps: Some(second_read - 1),
        },
    );
    assert_eq!(session.run_report().status, Status::NeedInput);
    let r = session.provide_report("10");
    assert_eq!(r.status, Status::Error);
    assert!(
        r.error
            .as_deref()
            .is_some_and(|e| e.contains("instruction quota")),
        "{:?}",
        r.error
    );
    assert!(
        r.output.contains("Dealing"),
        "output before the quota error is kept: {}",
        r.output
    );
}

#[test]
fn session_compile_error_is_a_status() {
    let mut session = Session::with_options("fn main( {", &RunOptions::default());
    let r = session.run_report();
    assert_eq!(r.status, Status::Error);
    assert!(
        r.error
            .as_deref()
            .is_some_and(|e| e.starts_with("compile error"))
    );
}
