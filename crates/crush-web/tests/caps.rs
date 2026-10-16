//! The capabilities a browser run gets (`browser_caps`): the same set
//! `crush-run` registers with no grant flags. The pure standard library,
//! `sys.args` and `sys.exit` work; nothing that reaches outside the VM is
//! there unless the page grants `time` or `env`, and a program that reaches an
//! ungranted capability is refused before it runs, as with `crush-run`.

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

fn granted(grants: &[&str]) -> RunOptions {
    RunOptions { grants: Some(grants.iter().map(|g| g.to_string()).collect()), ..Default::default() }
}

#[test]
fn nothing_outside_the_vm_is_registered() {
    for (source, grant) in [
        ("io.print(fs.cat(\"x\"))\n", "--fs: fs.cat"),
        ("io.print(time.now())\n", "--time: time.now"),
        ("io.print(env.get(\"HOME\"))\n", "--env: env.get"),
    ] {
        let result = execute_with_options(source, &RunOptions::default());
        assert!(!result.ok, "{source} ran: {}", result.output);
        assert!(result.error.unwrap_or_default().ends_with(grant), "{source}");
    }
}

/// As `crush-run` does: a program that can reach an ungranted capability is
/// refused before any of it runs, naming the flag that would grant each.
#[test]
fn ungranted_capabilities_are_refused_before_running() {
    let source = "io.print(\"first line\")\nio.print(fs.read(\"data.txt\"))\nio.print(time.now())\n";
    let want = "this program needs capabilities this run does not grant; nothing was run:\n  --fs: fs.read\n  --time: time.now";
    let result = execute_with_options(source, &RunOptions::default());
    assert!(!result.ok);
    assert_eq!(result.output, "", "nothing ran");
    assert_eq!(result.error.as_deref(), Some(want));

    let mut session = Session::with_options(source, &RunOptions::default());
    let report = session.run_report();
    assert_eq!(report.status, Status::Error);
    assert_eq!(report.output, "");
    assert_eq!(report.error.as_deref(), Some(want));

    // Granting one still refuses for the other.
    let result = execute_with_options(source, &granted(&["time"]));
    assert_eq!(result.output, "");
    assert!(result.error.unwrap_or_default().ends_with("nothing was run:\n  --fs: fs.read"));
}

#[test]
fn time_grant_gives_the_clock() {
    let source = "let t = time.now_ms()\nio.print(time.now() > 1700000000)\nio.print(t > 1700000000000)\n\
                  time.sleep(20)\nio.print(time.elapsed(t) >= 20)\nlet iso = time.now_iso()\n\
                  io.print(str.contains(iso, \"T\") && str.contains(iso, \"+00:00\"))\n";
    let result = execute_with_options(source, &granted(&["time"]));
    assert!(result.ok, "{:?}", result.error);
    assert_eq!(result.output, "true\ntrue\ntrue\ntrue\n");

    let bad = execute_with_options("time.sleep(-1)\n", &granted(&["time"]));
    // Word for word what `crush-run --time` reports.
    assert_eq!(
        bad.error.as_deref(),
        Some("runtime error: unknown capability: time.sleep: time.sleep: expected non-negative int milliseconds, got -1")
    );
}

#[test]
fn env_grant_reads_the_environment() {
    let source = "io.print(env.get(\"CRUSH_WEB_TEST_SURELY_UNSET\"))\n";
    let result = execute_with_options(source, &granted(&["env"]));
    assert!(result.ok, "{:?}", result.error);
    assert_eq!(result.output, "null\n");
}

/// In a browser the environment is empty: what `env -i crush-run --env` prints.
#[cfg(target_arch = "wasm32")]
#[test]
fn env_grant_in_a_browser_is_an_empty_environment() {
    let result = execute_with_options("io.print(env.all())\nio.print(env.home_dir())\n", &granted(&["env"]));
    assert_eq!(result.output, "{}\nnull\n");
}

#[test]
fn hosts_grants_stay_ungrantable() {
    for grant in ["fs", "process", "net", "polyglot.bash"] {
        let result = execute_with_options("io.print(1)\n", &granted(&[grant]));
        assert!(result.error.unwrap_or_default().contains("can't be granted in a browser"), "{grant}");
    }
}
