//! CRUSH-251 step 2: `MAKE_CLOSURE` / `CALL_VALUE` on both engines.

use crate::assembler::{assemble, disassemble};
use crate::vm::{FuncValue, Quotas, run};

/// Runs on the scheduler and the portable VM; they must agree, error included.
fn both(src: &str, caps: &[&str]) -> Result<String, String> {
    let prog = assemble(src, Some(caps), None).expect("assembly");
    let native = run(&prog, &Quotas::default()).map(|r| r.output).map_err(|e| e.to_string());
    let portable = crate::portable_vm::PortableVm::new(prog)
        .run()
        .map(|r| r.output)
        .map_err(|e| e.to_string());
    assert_eq!(portable, native, "portable VM and scheduler disagree");
    native
}

// `c - x` is order-sensitive: the captured value must arrive as the first
// (leading) parameter and the call's argument as the second.
const CAPTURED_LEADING: &str = "\
.func main
    PUSH 10
    PUSH 100
    MAKE_CLOSURE sub_from 1 1
    CALL_VALUE 1
    CAP_CALL \"io.print\" 1
    RET
.func sub_from
    STORE 0
    STORE 1
    LOAD 0
    LOAD 1
    SUB
    RET";

#[test]
fn captured_values_are_leading_parameters() {
    assert_eq!(both(CAPTURED_LEADING, &["io.print"]), Ok("90\n".to_string()));
}

#[test]
fn only_functions_are_callable() {
    let e = both(".func main\n    PUSH 5\n    CALL_VALUE 0\n    RET", &[]).unwrap_err();
    assert!(e.contains("int is not callable"), "{e}");
    // A string naming a function is not a function value.
    let e = both(".func main\n    PUSH_STR \"f\"\n    CALL_VALUE 0\n    RET\n.func f\n    PUSH 1\n    RET", &[])
        .unwrap_err();
    assert!(e.contains("str is not callable"), "{e}");
}

#[test]
fn arity_is_checked_against_the_function_value() {
    let src = ".func main\n    PUSH 1\n    PUSH 2\n    MAKE_CLOSURE id 1 0\n    CALL_VALUE 2\n    RET\n.func id\n    RET";
    let e = both(src, &[]).unwrap_err();
    assert!(e.contains("function `id` takes 1 argument(s), got 2"), "{e}");
}

#[test]
fn language_styled_printers_reject_function_values() {
    let src = ".func main\n    MAKE_CLOSURE f 0 0\n    CAP_CALL \"py.print\" 1\n    RET\n.func f\n    RET";
    let e = both(src, &["py.print"]).unwrap_err();
    assert!(e.contains("printing a function value (`f`)"), "{e}");
    let src = ".func main\n    MAKE_CLOSURE f 0 0\n    NEW_ARRAY 1\n    PUSH_STR \"js\"\n    CAP_CALL \"conv.text\" 2\n    RET\n.func f\n    RET";
    let e = both(src, &["conv.text"]).unwrap_err();
    assert!(e.contains("printing a function value (`f`)"), "{e}");
}

#[test]
fn closure_over_an_unknown_function_does_not_assemble() {
    let e = assemble(".func main\n    MAKE_CLOSURE nope 0 0\n    RET", None, None).unwrap_err();
    assert!(e.to_string().contains("closure over unknown function"), "{e}");
}

#[test]
fn disassembly_round_trips() {
    let text = disassemble(&assemble(CAPTURED_LEADING, Some(&["io.print"]), None).unwrap());
    assert!(text.contains("MAKE_CLOSURE sub_from 1 1"), "{text}");
    assert!(text.contains("CALL_VALUE 1"), "{text}");
}

#[test]
fn display_names() {
    let name = |n: &str| FuncValue { name: n.to_string(), arity: 0, captured: vec![] }.display_name().to_string();
    assert_eq!(name("__lambda_3_h"), "h");
    assert_eq!(name("__lambda_3_make_adder"), "make_adder");
    assert_eq!(name("__lambda_3"), "lambda");
    assert_eq!(name("fib"), "fib");
}
