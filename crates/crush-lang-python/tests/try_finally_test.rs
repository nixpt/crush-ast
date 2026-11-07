//! CRUSH-374: `finally` lowers by desugaring, so a jump out of a protected
//! block would skip it. Those are refused rather than silently wrong.

fn refused(source: &str) -> String {
    crush_lang_python::python_to_cast(source)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_else(|| panic!("should be refused:\n{source}"))
}

fn assert_refused(source: &str) {
    let msg = refused(source);
    assert!(msg.contains("finally"), "{msg}");
}

#[test]
fn return_in_try_with_finally_is_refused() {
    assert_refused("def f():\n    try:\n        return 1\n    finally:\n        print(9)\n");
}

#[test]
fn return_in_handler_with_finally_is_refused() {
    assert_refused(
        "def f():\n    try:\n        print(1)\n    except ValueError:\n        return 1\n    finally:\n        print(9)\n",
    );
}

#[test]
fn break_and_continue_across_finally_are_refused() {
    for jump in ["break", "continue"] {
        assert_refused(&format!(
            "for i in [1]:\n    try:\n        {jump}\n    finally:\n        print(9)\n"
        ));
    }
}

#[test]
fn return_nested_in_an_if_is_refused() {
    assert_refused(
        "def f(x):\n    try:\n        if x > 0:\n            return 1\n    finally:\n        print(9)\n",
    );
}

#[test]
fn return_in_finally_is_refused() {
    assert_refused("def f():\n    try:\n        print(1)\n    finally:\n        return 2\n");
}

#[test]
fn jumps_that_stay_inside_are_allowed() {
    crush_lang_python::python_to_cast(
        "for i in [1]:\n    try:\n        for j in [2]:\n            break\n    finally:\n        print(9)\n",
    )
    .unwrap();
}

#[test]
fn return_without_finally_is_unchanged() {
    crush_lang_python::python_to_cast(
        "def f():\n    try:\n        return 1\n    except ValueError:\n        return 2\n",
    )
    .unwrap();
}
