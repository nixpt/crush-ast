//! The SDK's `str.split` host cap splits like the VM built-in of the same
//! name: an empty delimiter gives the characters. It used to return
//! `["", "a", "b", ""]` for `split("ab", "")`, Rust's `str::split("")`.

use crush_lang_sdk::{HostCapsBuilder, Value};

fn split(s: &str, delim: &str) -> Vec<String> {
    let caps = HostCapsBuilder::new().stdlib(true).build();
    let cap = caps.get("str.split").expect("str.split registered");
    match cap.call(vec![Value::Str(s.into()), Value::Str(delim.into())]) {
        Ok(Some(Value::Array(a))) => a.borrow().iter().map(|v| v.to_string()).collect(),
        other => panic!("str.split({s:?}, {delim:?}) = {other:?}"),
    }
}

/// What the VM's built-in `str.split` prints, so the two can't drift.
fn vm_split(s: &str, delim: &str) -> String {
    let src = format!(
        ".func main\n    PUSH_STR {s:?}\n    PUSH_STR {delim:?}\n    CAP_CALL \"str.split\" 2\n    CAP_CALL \"io.print\" 1\n    RET\n"
    );
    let prog = crush_vm::assembler::assemble(&src, Some(&["io.print", "str.split"]), None).unwrap();
    crush_vm::run(&prog, &crush_vm::Quotas::default())
        .unwrap()
        .output
}

#[test]
fn split_matches_the_vm_builtin() {
    for (s, delim) in [
        ("ab", ""),
        ("a,b,,c", ","),
        ("", ","),
        ("", ""),
        ("héllo", ""),
    ] {
        let sdk = format!("[{}]\n", split(s, delim).join(", "));
        assert_eq!(sdk, vm_split(s, delim), "str.split({s:?}, {delim:?})");
    }
}

/// A non-string argument is turned into its text first, by the SDK cap and
/// the VM built-in alike (#425 asked whether the SDK refused it; it doesn't).
#[test]
fn non_string_arguments_are_coerced_like_the_vm() {
    let caps = HostCapsBuilder::new().stdlib(true).build();
    let cap = caps.get("str.split").expect("str.split registered");
    let sdk = match cap.call(vec![Value::Int(1020304), Value::Int(0)]) {
        Ok(Some(Value::Array(a))) => a.borrow().iter().map(|v| v.to_string()).collect::<Vec<_>>(),
        other => panic!("str.split(1020304, 0) = {other:?}"),
    };
    let src = ".func main\n    PUSH 1020304\n    PUSH 0\n    CAP_CALL \"str.split\" 2\n    CAP_CALL \"io.print\" 1\n    RET\n";
    let prog = crush_vm::assembler::assemble(src, Some(&["io.print", "str.split"]), None).unwrap();
    let vm = crush_vm::run(&prog, &crush_vm::Quotas::default())
        .unwrap()
        .output;
    assert_eq!(format!("[{}]\n", sdk.join(", ")), vm);
    assert_eq!(vm, "[1, 2, 3, 4]\n");
}
