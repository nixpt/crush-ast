//! #91: `@lang` blocks run in the browser only when the page grants
//! `polyglot.<lang>` AND supplies an executor; the executor decides where the
//! code runs, the capability gate is unchanged.

use std::cell::Cell;
use std::rc::Rc;

use crush_web::{execute_with_options, set_host_lang_executor, RunOptions};

fn opts(grants: &[&str]) -> RunOptions {
    RunOptions { grants: Some(grants.iter().map(|g| g.to_string()).collect()), ..Default::default() }
}

const PRINT_AROUND: &str = r#"
fn main() {
  print("before")
  @javascript {
    console.log("hello from js")
  }
  print("after")
}
"#;

#[test]
fn ungranted_block_is_refused_and_executor_not_called() {
    let calls = Rc::new(Cell::new(0));
    let c = calls.clone();
    set_host_lang_executor(Some(Rc::new(move |_, _, _| {
        c.set(c.get() + 1);
        Ok("ran".into())
    })));
    let r = execute_with_options(PRINT_AROUND, &RunOptions::default());
    set_host_lang_executor(None);
    assert!(!r.ok);
    let err = r.error.unwrap_or_default();
    assert!(err.contains("nothing was run") && err.contains("--polyglot: polyglot.javascript"), "{err}");
    assert_eq!(r.output, "", "refused before running, as crush-run does");
    assert_eq!(calls.get(), 0);
}

#[test]
fn granted_block_runs_through_the_executor() {
    let seen = Rc::new(std::cell::RefCell::new(String::new()));
    let s = seen.clone();
    set_host_lang_executor(Some(Rc::new(move |lang, code, _env| {
        *s.borrow_mut() = format!("{lang}:{}", code.contains("hello from js"));
        Ok("hello from js\n".into())
    })));
    let r = execute_with_options(PRINT_AROUND, &opts(&["polyglot.javascript"]));
    set_host_lang_executor(None);
    assert!(r.ok, "{:?}", r.error);
    assert_eq!(*seen.borrow(), "javascript:true");
    assert!(r.output.starts_with("before\n") && r.output.contains("hello from js") && r.output.ends_with("after\n"), "{:?}", r.output);
}

#[test]
fn executor_failure_keeps_earlier_output() {
    set_host_lang_executor(Some(Rc::new(|_, _, _| Err("ReferenceError: nope".into()))));
    let r = execute_with_options(PRINT_AROUND, &opts(&["polyglot.javascript"]));
    set_host_lang_executor(None);
    assert!(!r.ok);
    assert_eq!(r.output, "before\n");
    let err = r.error.unwrap_or_default();
    // The same error a native run reports when node exits non-zero.
    assert_eq!(err, "runtime error: @javascript block raised a runtime error: (at .crush line 4) ReferenceError: nope");
}

#[test]
fn bash_and_unknown_grants_are_refused() {
    for grant in ["polyglot.bash", "fs.read"] {
        let r = execute_with_options("fn main() { print(1) }", &opts(&[grant]));
        assert!(!r.ok);
        assert!(r.error.unwrap_or_default().contains("can't be granted in a browser"), "{grant}");
    }
}

/// With the `polyglot-javascript` feature, Crush variables go in (JSON in
/// `env`) and the block's output variable comes back typed (sentinel line).
#[cfg(feature = "polyglot-javascript")]
#[test]
fn marshaling_round_trip() {
    set_host_lang_executor(Some(Rc::new(|_, code, env| {
        assert!(code.contains("process.env"), "marshaling prologue present: {code}");
        let base: i64 = env.iter().find(|(k, _)| k == "base").map(|(_, v)| v.parse().unwrap()).expect("base passed in");
        // The sentinel carries an object keyed by output name (CRUSH-270).
        Ok(format!("\u{0}CRUSH_RESULT\u{0}{{\"result\": {}}}\n", base * 3))
    })));
    let r = execute_with_options(
        "fn main() {\n  let base = 5\n  @javascript {\n    var result = base * 3\n  }\n  print(result + 1)\n}\n",
        &opts(&["polyglot.javascript"]),
    );
    set_host_lang_executor(None);
    assert!(r.ok, "{:?}", r.error);
    assert_eq!(r.output.trim(), "16");
}
