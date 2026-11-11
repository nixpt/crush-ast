//! Tests for the capability dispatch domain.
//!
//! Auto-extracted from `tests.rs` as part of CRUSHTESTSSPLIT-1 (v2).
//!
//! Each fn preserves its original body verbatim; only the
//! section-banner organizer moved into a sub-file. Multi-line
//! banners are merged into a single classification.

use super::*;
use crate::assembler::{assemble, disassemble};
use crate::vm::{Quotas, Value, run};

// ── capabilities ─────────────────────────────────────────────────────────────

#[test]
fn cap_io_print() {
    let r = run_src_with_perms(
        "PUSH_STR \"hello\"\nCAP_CALL \"io.print\" 1\nHALT",
        &["io.print"],
    );
    assert_eq!(r.output, "hello\n");
}

#[test]
fn cap_io_read_is_registered_as_zero_arg_string_capability() {
    let spec = crate::capabilities()
        .get("io.read")
        .expect("io.read should be in the portable registry");
    assert_eq!(spec.argc, Some(0));
    assert!(spec.returns);
    assert!(!spec.privileged);
}

#[test]
fn cap_str_concat() {
    let r = run_src_with_perms(
        "PUSH_STR \"foo\"\nPUSH_STR \"bar\"\nCAP_CALL \"str.concat\" 2\nHALT",
        &["str.concat"],
    );
    assert_eq!(r.stack, vec![Value::Str("foobar".to_string())]);
}

#[test]
fn cap_str_len() {
    let r = run_src_with_perms(
        "PUSH_STR \"hello\"\nCAP_CALL \"str.len\" 1\nHALT",
        &["str.len"],
    );
    assert_eq!(r.stack, vec![Value::Int(5)]);
}

#[test]
fn cap_not_declared_errors() {
    let prog = assemble("PUSH_STR \"hi\"\nCAP_CALL \"io.print\" 1\nHALT", None, None).unwrap();
    assert!(run(&prog, &Quotas::default()).is_err());
}

// ── capability tests ──────────────────────────────────────────────────────────

#[test]
fn cap_str_contains() {
    let r = run_src_with_perms(
        r#"PUSH_STR "hello world"
    PUSH_STR "world"
    CAP_CALL "str.contains" 2
    HALT"#,
        &["str.contains"],
    );
    assert_eq!(r.stack, vec![Value::Bool(true)]);
}

#[test]
fn cap_str_split() {
    let r = run_src_with_perms(
        r#"PUSH_STR "a,b,c"
    PUSH_STR ","
    CAP_CALL "str.split" 2
    HALT"#,
        &["str.split"],
    );
    if let Some(Value::Array(arr)) = r.stack.first() {
        assert_eq!(arr.borrow().len(), 3);
    } else {
        panic!("expected array");
    }
}

#[test]
fn cap_str_replace() {
    let r = run_src_with_perms(
        r#"PUSH_STR "hello world"
    PUSH_STR "world"
    PUSH_STR "there"
    CAP_CALL "str.replace" 3
    HALT"#,
        &["str.replace"],
    );
    assert_eq!(r.stack, vec![Value::Str("hello there".to_string())]);
}

#[test]
fn cap_str_join() {
    let r = run_src_with_perms(
        r#"PUSH_STR "a"
    PUSH_STR "b"
    NEW_ARRAY 2
    PUSH_STR ","
    CAP_CALL "str.join" 2
    HALT"#,
        &["str.join"],
    );
    assert_eq!(r.stack, vec![Value::Str("a,b".to_string())]);
}

#[test]
fn cap_make_range() {
    let r = run_src_with_perms(
        "PUSH 0\nPUSH 5\nCAP_CALL \"make_range\" 2\nHALT",
        &["make_range"],
    );
    if let Some(Value::Array(arr)) = r.stack.first() {
        assert_eq!(arr.borrow().len(), 5);
    } else {
        panic!("expected array");
    }
}

#[test]
fn cap_conv_chr_ord_round_trip_unicode() {
    let r = run_src_with_perms(
        "PUSH 233\nCAP_CALL \"conv.chr\" 1\nCAP_CALL \"conv.ord\" 1\nHALT",
        &["conv.chr", "conv.ord"],
    );
    assert_eq!(r.stack, vec![Value::Int(233)]);

    let r = run_src_with_perms(
        "PUSH 128512\nCAP_CALL \"conv.chr\" 1\nCAP_CALL \"conv.ord\" 1\nHALT",
        &["conv.chr", "conv.ord"],
    );
    assert_eq!(r.stack, vec![Value::Int(128512)]);
}

#[test]
fn cap_conv_chr_ord_reject_invalid_values() {
    for source in [
        "PUSH -1\nCAP_CALL \"conv.chr\" 1\nHALT",
        "PUSH 55296\nCAP_CALL \"conv.chr\" 1\nHALT",
        "PUSH 1114112\nCAP_CALL \"conv.chr\" 1\nHALT",
        "PUSH_STR \"\"\nCAP_CALL \"conv.ord\" 1\nHALT",
        "PUSH_STR \"ab\"\nCAP_CALL \"conv.ord\" 1\nHALT",
    ] {
        let program = assemble(source, Some(&["conv.chr", "conv.ord"]), Some("test")).unwrap();
        assert!(run(&program, &Quotas::default()).is_err(), "expected rejection: {source}");
    }
}

fn map_is_truthy_only_when_non_empty() {
    assert!(!Value::new_map(std::collections::HashMap::new()).is_truthy());
    let mut m = std::collections::HashMap::new();
    m.insert("x".to_string(), Value::Int(1));
    assert!(Value::new_map(m).is_truthy());
}

#[test]
fn cap_io_read_line_is_registered_as_zero_arg_capability() {
    let spec = crate::capabilities()
        .get("io.read_line")
        .expect("io.read_line should be in the portable registry");
    assert_eq!(spec.argc, Some(0));
    assert!(spec.returns);
}

// ── CRUSH-241: `io.eprint` is stdout's stderr twin ──────────────────────────

#[test]
fn cap_io_eprint_is_registered_as_variadic_no_return() {
    let spec = crate::capabilities()
        .get("io.eprint")
        .expect("io.eprint should be in the portable registry");
    assert_eq!(spec.argc, None, "io.eprint takes any number of arguments, like io.print");
    assert!(!spec.returns, "io.eprint leaves nothing on the stack");
    assert!(!spec.privileged, "writing to stderr needs no grant, like io.print");
}

#[test]
fn cap_io_eprint_feeds_the_stderr_channel_not_stdout() {
    let r = run_src_with_perms(
        "PUSH_STR \"out\"\nCAP_CALL \"io.print\" 1\nPUSH_STR \"diag\"\nPUSH_STR \" 1\"\nCAP_CALL \"io.eprint\" 2\nHALT",
        &["io.print", "io.eprint"],
    );
    assert_eq!(r.output, "out\n", "eprint text must not appear in stdout");
    assert_eq!(r.stderr, "diag 1\n", "eprint concatenates its args and adds one newline");
}

#[test]
fn portable_vm_io_eprint_matches_the_scheduler() {
    let source = "PUSH_STR \"out\"\nCAP_CALL \"io.print\" 1\nPUSH_STR \"diag\"\nCAP_CALL \"io.eprint\" 1\nHALT";
    let program = assemble(source, Some(&["io.print", "io.eprint"]), Some("t")).unwrap();
    let mut vm = crate::PortableVm::new(program);
    let result = vm.run().expect("run");
    assert_eq!(result.output, "out\n");
    assert_eq!(result.stderr, "diag\n");
    assert_eq!(vm.take_output(), "out\n");
    assert_eq!(vm.take_stderr(), "diag\n");
}

// ── CRUSH-241: a registered capability that fails is not "unknown" ──────────

#[test]
fn failing_registered_capability_is_reported_as_cap_and_message() {
    struct Boom;
    impl crate::host::HostCap for Boom {
        fn spec(&self) -> crate::host::HostCapSpec {
            crate::host::HostCapSpec { name: "test.boom".to_string(), argc: None, returns: false }
        }
        fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
            Err("test.boom target.txt: permission denied".to_string())
        }
    }
    let mut caps = crate::host::HostCaps::new();
    caps.register(Box::new(Boom));
    let program = assemble("CAP_CALL \"test.boom\" 0\nHALT", Some(&["test.boom"]), None).unwrap();
    let error = crate::vm::run_with_caps(&program, &Quotas::default(), Some(&caps)).unwrap_err();
    assert!(
        !matches!(error, crate::vm::VmError::UnknownCap(_)),
        "a registered capability that failed is a CapFailed, got: {error:?}"
    );
    assert_eq!(error.to_string(), "test.boom: test.boom target.txt: permission denied");
}

// ── CRUSH-391: `pop` (the a.pop() method form) and str.ord / str.chr ────────

#[test]
fn cap_pop_is_registered_as_one_arg_returning() {
    let spec = crate::capabilities()
        .get("pop")
        .expect("pop should be in the portable registry");
    assert_eq!(spec.argc, Some(1));
    assert!(spec.returns);
    assert!(!spec.privileged);
}

#[test]
fn cap_pop_removes_and_returns_the_last_element() {
    let r = run_src_with_perms(
        "PUSH_STR \"a\"\nPUSH_STR \"b\"\nNEW_ARRAY 2\nCAP_CALL \"pop\" 1\nHALT",
        &["pop"],
    );
    // Only the element stays: the receiver is consumed as the argument and
    // mutated in place (the program's variable still references it), and the
    // removed element is the expression's value — nothing is left under it.
    assert_eq!(r.stack, vec![Value::Str("b".to_string())]);
}

#[test]
fn cap_pop_on_an_empty_array_returns_null() {
    let r = run_src_with_perms("NEW_ARRAY 0\nCAP_CALL \"pop\" 1\nHALT", &["pop"]);
    assert_eq!(r.stack, vec![Value::Null]);
}

#[test]
fn cap_pop_rejects_a_non_array_receiver() {
    let program = assemble("PUSH 5\nCAP_CALL \"pop\" 1\nHALT", Some(&["pop"]), Some("t")).unwrap();
    assert!(run(&program, &Quotas::default()).is_err());
}

#[test]
fn portable_vm_pop_matches_the_scheduler() {
    let source = "PUSH 1\nPUSH 2\nNEW_ARRAY 2\nCAP_CALL \"pop\" 1\nHALT";
    let program = assemble(source, Some(&["pop"]), Some("t")).unwrap();
    let mut vm = crate::PortableVm::new(program.clone());
    let via_portable = vm.run().expect("portable vm run");
    let via_scheduler = run(&program, &Quotas::default()).expect("scheduler run");
    assert_eq!(via_portable.stack, via_scheduler.stack, "pop must not diverge between the CVM1 engines");
    assert_eq!(via_portable.stack, vec![Value::Int(2)]);
}

#[test]
fn cap_str_ord_chr_are_registered_one_arg_returning() {
    for name in ["str.ord", "str.chr"] {
        let spec = crate::capabilities().get(name)
            .unwrap_or_else(|| panic!("{name} should be in the portable registry"));
        assert_eq!(spec.argc, Some(1));
        assert!(spec.returns);
        assert!(!spec.privileged);
    }
}

#[test]
fn cap_str_ord_chr_round_trip() {
    let r = run_src_with_perms("PUSH_STR \"A\"\nCAP_CALL \"str.ord\" 1\nHALT", &["str.ord"]);
    assert_eq!(r.stack, vec![Value::Int(65)]);
    let r = run_src_with_perms("PUSH 66\nCAP_CALL \"str.chr\" 1\nHALT", &["str.chr"]);
    assert_eq!(r.stack, vec![Value::Str("B".to_string())]);
    let r = run_src_with_perms("PUSH_STR \"é\"\nCAP_CALL \"str.ord\" 1\nCAP_CALL \"str.chr\" 1\nHALT", &["str.ord", "str.chr"]);
    assert_eq!(r.stack, vec![Value::Str("é".to_string())]);
}

#[test]
fn cap_str_ord_chr_match_conv_ord_chr_exactly() {
    // The str.* names are aliases of the conv.* capabilities (CRUSH-391):
    // same accepted inputs, same results, same rejections — one shared
    // implementation, not a second one.
    for (ord_push, chr_push) in [
        ("PUSH_STR \"A\"", "PUSH 65"),
        ("PUSH_STR \"é\"", "PUSH 233"),
        ("PUSH_STR \"󰀀\"", "PUSH 130048"),
    ] {
        let via_conv = run_src_with_perms(&format!("{ord_push}\nCAP_CALL \"conv.ord\" 1\nHALT"), &["conv.ord"]);
        let via_str = run_src_with_perms(&format!("{ord_push}\nCAP_CALL \"str.ord\" 1\nHALT"), &["str.ord"]);
        assert_eq!(via_conv.stack, via_str.stack, "str.ord must match conv.ord for {ord_push}");
        let via_conv = run_src_with_perms(&format!("{chr_push}\nCAP_CALL \"conv.chr\" 1\nHALT"), &["conv.chr"]);
        let via_str = run_src_with_perms(&format!("{chr_push}\nCAP_CALL \"str.chr\" 1\nHALT"), &["str.chr"]);
        assert_eq!(via_conv.stack, via_str.stack, "str.chr must match conv.chr for {chr_push}");
    }
    for source in [
        "PUSH_STR \"\"\nCAP_CALL \"str.ord\" 1\nHALT",
        "PUSH_STR \"ab\"\nCAP_CALL \"str.ord\" 1\nHALT",
        "PUSH -1\nCAP_CALL \"str.chr\" 1\nHALT",
        "PUSH 1114112\nCAP_CALL \"str.chr\" 1\nHALT",
    ] {
        let program = assemble(source, Some(&["str.ord", "str.chr"]), Some("test")).unwrap();
        assert!(run(&program, &Quotas::default()).is_err(), "expected rejection: {source}");
    }
}
