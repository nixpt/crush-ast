//! The capabilities a browser run gets (`browser_caps`): the same set
//! `crush-run` registers with no grant flags. The pure standard library,
//! `sys.args` and `sys.exit` work; nothing that reaches outside the VM is
//! there.

use crush_web::{RunOptions, Session, Status, execute_with_options};

/// What a corpus program's `// expect:` lines say it prints.
#[cfg(feature = "stdlib")]
fn expected(source: &str) -> String {
    source
        .lines()
        .filter_map(|l| l.strip_prefix("// expect: "))
        .map(|l| format!("{l}\n"))
        .collect()
}

#[cfg(feature = "stdlib")]
fn assert_runs_like_native(source: &str) {
    let result = execute_with_options(source, &RunOptions::default());
    assert!(
        result.ok,
        "{:?}\noutput so far: {}",
        result.error, result.output
    );
    assert_eq!(result.output, expected(source));
}

#[cfg(feature = "stdlib")]
#[test]
fn math_library_works() {
    assert_runs_like_native(include_str!("../../../examples/crush/math_test.crush"));
}

#[cfg(feature = "stdlib")]
#[test]
fn system_library_works() {
    assert_runs_like_native(include_str!("../../../examples/crush/test_sbl.crush"));
}

#[test]
fn sys_args_returns_the_args_option() {
    let source = "let a = sys.args()\nio.print(len(a))\nio.print(a[1])\n";
    let options = RunOptions {
        args: Some(vec!["one".into(), "two words".into()]),
        ..Default::default()
    };
    let result = execute_with_options(source, &options);
    assert!(result.ok, "{:?}", result.error);
    assert_eq!(result.output, "2\ntwo words\n");

    let none = execute_with_options("io.print(len(sys.args()))\n", &RunOptions::default());
    assert_eq!(none.output, "0\n");
}

#[test]
fn sys_exit_sets_exit_code_and_keeps_output() {
    let source = "io.print(\"before\")\nsys.exit(3)\nio.print(\"after\")\n";
    let result = execute_with_options(source, &RunOptions::default());
    assert!(!result.ok);
    assert_eq!(result.exit_code, Some(3));
    assert_eq!(result.error, None);
    assert_eq!(result.output, "before\n");

    let zero = execute_with_options("sys.exit(0)\nio.print(\"after\")\n", &RunOptions::default());
    assert!(zero.ok);
    assert_eq!(zero.exit_code, Some(0));
    assert_eq!(zero.output, "");
}

#[test]
fn session_reports_sys_exit_as_done() {
    let source = "let x = io.read()\nio.print(\"got \" + x)\nsys.exit(4)\n";
    let mut session = Session::with_options(source, &RunOptions::default());
    assert_eq!(session.run_report().status, Status::NeedInput);
    let report = session.provide_report("hi");
    assert_eq!(report.status, Status::Done, "{:?}", report.error);
    assert_eq!(report.exit_code, Some(4));
    assert_eq!(report.output, "got hi\n");
    // Later calls repeat the final status.
    assert_eq!(session.run_report().exit_code, Some(4));
}

#[test]
fn nothing_outside_the_vm_is_registered() {
    for source in [
        "io.print(fs.cat(\"x\"))\n",
        "io.print(time.now())\n",
        "io.print(env.get(\"HOME\"))\n",
    ] {
        let result = execute_with_options(source, &RunOptions::default());
        assert!(!result.ok, "{source} ran: {}", result.output);
    }
}
