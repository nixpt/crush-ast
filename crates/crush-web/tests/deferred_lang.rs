//! `deferLang`: a `Session` hands each granted `@lang` block to the page
//! (`status: "need_lang"`) instead of calling the executor during the run, so
//! the page can run it asynchronously (a JavaScript block's promises and
//! timers) and continue with `provide_lang` / `fail_lang`.

use crush_web::{RunOptions, Session, Status, execute_with_options};

fn deferred(grants: &[&str]) -> RunOptions {
    RunOptions {
        grants: Some(grants.iter().map(|g| g.to_string()).collect()),
        defer_lang: Some(true),
        ..Default::default()
    }
}

const AROUND: &str = "fn main() {\n  print(\"before\")\n  @javascript {\n    console.log(\"from js\")\n  }\n  print(\"after\")\n}\n";

#[test]
fn a_granted_block_pauses_the_run_until_the_page_provides_its_output() {
    let mut s = Session::with_options(AROUND, &deferred(&["polyglot.javascript"]));
    let r = s.run_report();
    assert_eq!(r.status, Status::NeedLang, "{:?}", r.error);
    assert_eq!(r.output, "before\n", "what ran before the block is out");
    let lang = r.lang.expect("the block to run");
    assert_eq!(lang.lang, "javascript");
    assert!(lang.code.contains("from js"), "{}", lang.code);
    let r = s.lang_report(Ok("from js\n".to_string()));
    assert_eq!(r.status, Status::Done, "{:?}", r.error);
    assert_eq!(r.output, "from jsafter\n", "the block's output trimmed, as EXEC_LANG prints it");
    assert!(r.lang.is_none());
}

#[test]
fn a_failed_block_fails_the_run_as_a_synchronous_executor_does() {
    let mut s = Session::with_options(AROUND, &deferred(&["polyglot.javascript"]));
    assert_eq!(s.run_report().status, Status::NeedLang);
    let r = s.lang_report(Err("ReferenceError: nope".to_string()));
    assert_eq!(r.status, Status::Error);
    assert_eq!(
        r.error.as_deref(),
        Some("runtime error: @javascript block raised a runtime error: (at .crush line 3) ReferenceError: nope")
    );
    assert_eq!(s.transcript(), "before\n", "earlier output is kept");
}

#[test]
fn an_ungranted_block_is_refused_before_anything_runs() {
    let mut s = Session::with_options(AROUND, &deferred(&[]));
    let r = s.run_report();
    assert_eq!(r.status, Status::Error);
    assert!(r.error.unwrap_or_default().contains("--polyglot: polyglot.javascript"));
    assert_eq!(r.output, "");
}

#[test]
fn a_block_in_a_loop_pauses_once_per_iteration() {
    let src = "fn main() {\n  for i in 0..3 {\n    @javascript {\n      console.log(\"x\")\n    }\n  }\n  print(\"end\")\n}\n";
    let mut s = Session::with_options(src, &deferred(&["polyglot.javascript"]));
    let mut r = s.run_report();
    let mut pauses = 0;
    while r.status == Status::NeedLang {
        pauses += 1;
        r = s.lang_report(Ok(format!("{pauses}\n")));
    }
    assert_eq!(r.status, Status::Done, "{:?}", r.error);
    assert_eq!(pauses, 3);
    assert_eq!(s.transcript(), "123end\n");
}

#[test]
fn providing_a_result_with_no_block_waiting_is_an_error() {
    let mut s = Session::with_options("fn main() { print(1) }", &deferred(&[]));
    assert_eq!(s.run_report().status, Status::Done);
    let r = s.lang_report(Ok(String::new()));
    assert_eq!(r.status, Status::Error);
    assert_eq!(r.error.as_deref(), Some("no @lang block is waiting to run"));
}

#[test]
fn execute_with_cannot_defer() {
    let r = execute_with_options(AROUND, &deferred(&["polyglot.javascript"]));
    assert!(!r.ok);
    assert_eq!(r.output, "before\n");
    assert!(r.error.unwrap_or_default().contains("deferLang needs a Session"));
}

/// With the `polyglot-javascript` feature, the block's inputs come with the
/// pause (JSON by name, as a subprocess gets them) and its `->` value comes
/// back typed through the result sentinel, as with a synchronous executor.
#[cfg(feature = "polyglot-javascript")]
#[test]
fn inputs_go_out_with_the_pause_and_outputs_come_back_typed() {
    let src = "fn main() {\n  let base = 5\n  @javascript -> result {\n    const result = base * 3\n  }\n  print(result + 1)\n}\n";
    let mut s = Session::with_options(src, &deferred(&["polyglot.javascript"]));
    let r = s.run_report();
    assert_eq!(r.status, Status::NeedLang, "{:?}", r.error);
    let lang = r.lang.unwrap();
    assert!(lang.code.contains("process.env"), "marshaling prologue present: {}", lang.code);
    assert_eq!(lang.env.get("base").map(String::as_str), Some("5"));
    let r = s.lang_report(Ok("\u{0}CRUSH_RESULT\u{0}{\"result\": 15}\n".to_string()));
    assert_eq!(r.status, Status::Done, "{:?}", r.error);
    assert_eq!(s.transcript(), "16\n");
}
