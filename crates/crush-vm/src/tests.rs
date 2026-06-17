use crate::assembler::{assemble, disassemble};
use crate::vm::{Quotas, Value, run};

fn run_src(src: &str) -> crate::vm::VmResult {
    let prog = assemble(src, None, None).expect("assembly");
    run(&prog, &Quotas::default()).expect("vm run")
}

fn run_src_with_perms(src: &str, perms: &[&str]) -> crate::vm::VmResult {
    let prog = assemble(src, Some(perms), None).expect("assembly");
    run(&prog, &Quotas::default()).expect("vm run")
}

// ── arithmetic ────────────────────────────────────────────────────────────────

#[test]
fn push_and_halt() {
    let r = run_src("PUSH 42\nHALT");
    assert_eq!(r.stack, vec![Value::Int(42)]);
    assert!(r.halted);
}

#[test]
fn add_integers() {
    let r = run_src("PUSH 3\nPUSH 4\nADD\nHALT");
    assert_eq!(r.stack, vec![Value::Int(7)]);
}

#[test]
fn sub_mul_div() {
    let r = run_src("PUSH 10\nPUSH 3\nSUB\nHALT");
    assert_eq!(r.stack, vec![Value::Int(7)]);
    let r = run_src("PUSH 6\nPUSH 7\nMUL\nHALT");
    assert_eq!(r.stack, vec![Value::Int(42)]);
    let r = run_src("PUSH 10\nPUSH 3\nDIV\nHALT");
    assert_eq!(r.stack, vec![Value::Int(3)]);
}

#[test]
fn modulo() {
    let r = run_src("PUSH 10\nPUSH 3\nMOD\nHALT");
    assert_eq!(r.stack, vec![Value::Int(1)]);
}

#[test]
fn mod_negative_values() {
    let r = run_src("PUSH -7\nPUSH 3\nMOD\nHALT");
    assert_eq!(r.stack, vec![Value::Int(-1)]);  // Rust-style truncation: -7 - 3*(-2) = -1
}

#[test]
fn float_push() {
    let r = run_src("PUSH_F64 3.14\nHALT");
    assert!(matches!(r.stack.first(), Some(Value::Float(f)) if (f - 3.14).abs() < 1e-10));
}

#[test]
fn comparisons() {
    let r = run_src("PUSH 3\nPUSH 5\nLT\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH 5\nPUSH 3\nGT\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH 5\nPUSH 5\nEQ\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
}

#[test]
fn logical_not() {
    let r = run_src("PUSH 0\nNOT\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH 42\nNOT\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(false)]);
}

#[test]
fn negate() {
    let r = run_src("PUSH 42\nNEG\nHALT");
    assert_eq!(r.stack, vec![Value::Int(-42)]);
    let r = run_src("PUSH_F64 3.5\nNEG\nHALT");
    assert!(matches!(r.stack.first(), Some(Value::Float(f)) if (*f - (-3.5)).abs() < 1e-10));
}

#[test]
fn extended_comparisons() {
    let r = run_src("PUSH 3\nPUSH 5\nNE\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH 5\nPUSH 5\nNE\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(false)]);
    let r = run_src("PUSH 3\nPUSH 5\nLE\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH 5\nPUSH 5\nLE\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH 6\nPUSH 5\nLE\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(false)]);
    let r = run_src("PUSH 6\nPUSH 5\nGE\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH 5\nPUSH 5\nGE\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH 3\nPUSH 5\nGE\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(false)]);
}

#[test]
fn logical_and_or() {
    let r = run_src("PUSH 1\nPUSH 42\nAND\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH 0\nPUSH 42\nAND\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(false)]);
    let r = run_src("PUSH 0\nPUSH 42\nOR\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH 0\nPUSH 0\nOR\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(false)]);
}

// ── stack ops ────────────────────────────────────────────────────────────────

#[test]
fn dup_and_swap() {
    let r = run_src("PUSH 1\nPUSH 2\nSWAP\nHALT");
    assert_eq!(r.stack, vec![Value::Int(2), Value::Int(1)]);
    let r = run_src("PUSH 7\nDUP\nHALT");
    assert_eq!(r.stack, vec![Value::Int(7), Value::Int(7)]);
}

#[test]
fn pop_removes_top() {
    let r = run_src("PUSH 1\nPUSH 2\nPOP\nHALT");
    assert_eq!(r.stack, vec![Value::Int(1)]);
}

// ── control flow ──────────────────────────────────────────────────────────────

#[test]
fn unconditional_jump() {
    // Jump over a PUSH that would put 99 on the stack.
    let r = run_src("JMP done\nPUSH 99\ndone:\nPUSH 1\nHALT");
    assert_eq!(r.stack, vec![Value::Int(1)]);
}

#[test]
fn jz_taken() {
    let r = run_src("PUSH 0\nJZ end\nPUSH 99\nend:\nPUSH 1\nHALT");
    assert_eq!(r.stack, vec![Value::Int(1)]);
}

#[test]
fn jz_not_taken() {
    let r = run_src("PUSH 5\nJZ skip\nPUSH 42\nskip:\nHALT");
    assert_eq!(r.stack, vec![Value::Int(42)]);
}

#[test]
fn countdown_loop() {
    // count down from 3 to 0 using JNZ.
    let src = "PUSH 3\nloop:\nPUSH 1\nSUB\nDUP\nJNZ loop\nHALT";
    let r = run_src(src);
    assert_eq!(r.stack, vec![Value::Int(0)]);
}

// ── slots (memory) ───────────────────────────────────────────────────────────

#[test]
fn store_and_load() {
    let r = run_src("PUSH 42\nSTORE 0\nLOAD 0\nHALT");
    assert_eq!(r.stack, vec![Value::Int(42)]);
}

// ── strings ──────────────────────────────────────────────────────────────────

#[test]
fn push_str() {
    let r = run_src("PUSH_STR \"hello\"\nHALT");
    assert_eq!(r.stack, vec![Value::Str("hello".to_string())]);
}

#[test]
fn push_null() {
    let r = run_src("PUSH_NULL\nHALT");
    assert_eq!(r.stack, vec![Value::Null]);
}

// ── arrays ────────────────────────────────────────────────────────────────────

#[test]
fn new_array() {
    let r = run_src("PUSH 1\nPUSH 2\nPUSH 3\nNEW_ARRAY 3\nHALT");
    assert_eq!(
        r.stack,
        vec![Value::Array(vec![
            Value::Int(1),
            Value::Int(2),
            Value::Int(3)
        ])]
    );
}

#[test]
fn arr_get_and_len() {
    let r = run_src("PUSH 10\nPUSH 20\nNEW_ARRAY 2\nARR_LEN\nHALT");
    assert_eq!(r.stack, vec![Value::Int(2)]);
    let r = run_src("PUSH 10\nPUSH 20\nNEW_ARRAY 2\nPUSH 0\nARR_GET\nHALT");
    assert_eq!(r.stack, vec![Value::Int(10)]);
}

#[test]
fn arr_set() {
    let r =
        run_src("PUSH 10\nPUSH 20\nNEW_ARRAY 2\nPUSH 0\nPUSH 99\nARR_SET\nPUSH 0\nARR_GET\nHALT");
    assert_eq!(r.stack, vec![Value::Int(99)]);
}

// ── capabilities ─────────────────────────────────────────────────────────────

#[test]
fn cap_io_print() {
    let r = run_src_with_perms(
        "PUSH_STR \"hello\"\nCAP_CALL \"io.print\" 1\nHALT",
        &["io.print"],
    );
    assert_eq!(r.output, "hello");
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

// ── functions (v2) ───────────────────────────────────────────────────────────

#[test]
fn function_call_and_ret() {
    let src = "\
.func main
    PUSH 1
    CALL double
    HALT
.func double
    PUSH 2
    MUL
    RET
";
    let r = run_src(src);
    assert_eq!(r.stack, vec![Value::Int(2)]);
}

// ── binary round-trip ────────────────────────────────────────────────────────

#[test]
fn blob_roundtrip() {
    let prog = assemble("PUSH 42\nHALT", None, None).unwrap();
    let blob = prog.to_blob();
    let prog2 = crate::bytecode::Program::from_blob(&blob).unwrap();
    assert_eq!(prog2.code, prog.code);
    assert_eq!(prog2.consts, prog.consts);
}

// ── disassembler ─────────────────────────────────────────────────────────────

#[test]
fn disassemble_roundtrip() {
    let src = "PUSH 5\nPUSH 3\nADD\nHALT\n";
    let prog = assemble(src, None, None).unwrap();
    let text = disassemble(&prog);
    let prog2 = assemble(&text, None, None).unwrap();
    assert_eq!(prog.code, prog2.code);
}

// ── step quota ───────────────────────────────────────────────────────────────

#[test]
fn step_quota_triggers() {
    let prog = assemble("loop:\nJMP loop", None, None).unwrap();
    let quotas = Quotas {
        max_steps: 10,
        ..Default::default()
    };
    assert!(run(&prog, &quotas).is_err());
}

// ── new types: Bool, Map, Error, Bytes, ARR_PUSH/POP ─────────────────────────

#[test]
fn push_bool_works() {
    let r = run_src("PUSH_BOOL 1\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH_BOOL 0\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(false)]);
}

#[test]
fn new_obj_creates_empty_map() {
    let r = run_src("NEW_OBJ\nHALT");
    assert_eq!(r.stack.len(), 1);
    assert!(matches!(r.stack[0], Value::Map(_)));
}

#[test]
fn set_field_and_get_field() {
    // NEW_OBJ, DUP, PUSH_STR "hello", SET_FIELD "greeting", GET_FIELD "greeting"
    let r = run_src(
        r#"NEW_OBJ
    DUP
    PUSH_STR "hello"
    SET_FIELD "greeting"
    GET_FIELD "greeting"
    HALT"#,
    );
    assert_eq!(r.stack.len(), 2);
    assert!(matches!(r.stack[0], Value::Map(_)));
    assert_eq!(r.stack[1], Value::Str("hello".to_string()));
}

#[test]
fn get_field_missing_returns_null() {
    let r = run_src("NEW_OBJ\nGET_FIELD \"missing\"\nHALT");
    assert_eq!(r.stack, vec![Value::Null]);
}

#[test]
fn map_type_name() {
    let r = run_src("NEW_OBJ\nHALT");
    assert_eq!(r.stack[0].type_name(), "map");
}

#[test]
fn throw_basic() {
    let prog = assemble("PUSH_STR \"oops\"\nTHROW\nHALT", None, None).unwrap();
    let result = run(&prog, &Quotas::default());
    assert!(result.is_err());
}

#[test]
fn enter_try_and_exit_try_no_error() {
    // try { push 1 } catch { push 2 }
    let r = run_src("ENTER_TRY handler\nPUSH 1\nEXIT_TRY\nJMP done\nhandler:\nPUSH 2\ndone:\nHALT");
    assert_eq!(r.stack, vec![Value::Int(1)]);
}

#[test]
fn try_catch_catches_throw() {
    // try { throw "err" } catch { pop error, push 99 }
    // THROW pushes the error value onto the stack before jumping to handler
    let r = run_src(
        "ENTER_TRY handler\nPUSH_STR \"err\"\nTHROW\nEXIT_TRY\nJMP done\nhandler:\nPOP\nPUSH 99\ndone:\nHALT",
    );
    assert_eq!(r.stack, vec![Value::Int(99)]);
}

#[test]
fn throw_error_value_on_stack_in_handler() {
    // try { throw "msg" } catch { the error value is already on stack }
    let r = run_src(
        "ENTER_TRY handler\nPUSH_STR \"msg\"\nTHROW\nEXIT_TRY\nJMP done\nhandler:\nHALT\ndone:\nHALT",
    );
    // After THROW, the error "msg" is pushed onto the stack for handler
    assert_eq!(r.stack, vec![Value::Str("msg".to_string())]);
}

#[test]
fn bool_type_name() {
    assert_eq!(Value::Bool(true).type_name(), "bool");
    assert_eq!(Value::Bool(false).type_name(), "bool");
}

#[test]
fn bool_as_text() {
    assert_eq!(Value::Bool(true).as_text(), "true");
    assert_eq!(Value::Bool(false).as_text(), "false");
}

#[test]
fn error_type_name_and_text() {
    let e = Value::Error("test error".to_string());
    assert_eq!(e.type_name(), "error");
    assert!(e.as_text().contains("test error"));
    assert!(e.is_truthy());
}

#[test]
fn bytes_type_name_and_text() {
    let b = Value::Bytes(vec![1, 2, 3]);
    assert_eq!(b.type_name(), "bytes");
    assert!(b.as_text().contains("3 bytes"));
    assert!(b.is_truthy());
    assert!(!Value::Bytes(vec![]).is_truthy());
}

#[test]
fn bool_is_not_numeric() {
    assert!(!Value::Bool(true).is_numeric());
    assert!(!Value::Bool(false).is_numeric());
}

#[test]
fn arr_push_and_arr_pop() {
    // Build [1, 2] without using DUP (so stack is clean):
    // NEW_ARRAY 0, PUSH 1, ARR_PUSH, PUSH 2, ARR_PUSH
    // But ARR_PUSH needs the array on the stack too.
    // With NEW_ARRAY → DUP → PUSH → ARR_PUSH, the DUP leaves a copy.
    // After the sequence, the last stack item is the final array.
    let r = run_src("NEW_ARRAY 0\nDUP\nPUSH 1\nARR_PUSH\nDUP\nPUSH 2\nARR_PUSH\nHALT");
    let last = r.stack.last().expect("should have a value");
    if let Value::Array(arr) = last {
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0], Value::Int(1));
        assert_eq!(arr[1], Value::Int(2));
    } else {
        panic!("expected array, got {:?}", last);
    }
}

#[test]
fn arr_pop_removes_last() {
    let r = run_src(
        "NEW_ARRAY 0\nDUP\nPUSH 1\nARR_PUSH\nDUP\nPUSH 2\nARR_PUSH\nDUP\nPUSH 3\nARR_PUSH\nARR_POP\nPOP\nARR_POP\nHALT",
    );
    // Stack after NEW_ARRAY + 3 pushes + 2 pops + 1 pop:
    // After all ARR_PUSH ops: stack has old copies + final array
    // Last element should be 2 (popped value from second ARR_POP)
    // Second-to-last should be [1] (array after popping 2)
    let len = r.stack.len();
    assert!(len >= 2, "expected at least 2 values, got {}", len);
    if let Value::Int(v) = &r.stack[len - 1] {
        assert_eq!(*v, 2);
    } else {
        panic!("expected Int, got {:?}", r.stack[len - 1]);
    }
    if let Value::Array(arr) = &r.stack[len - 2] {
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0], Value::Int(1));
    } else {
        panic!("expected Array, got {:?}", r.stack[len - 2]);
    }
}

#[test]
// ── bitwise ops ───────────────────────────────────────────────────────────────

#[test]
fn bitwise_and_or_xor() {
    let r = run_src("PUSH 12\nPUSH 10\nBITAND\nHALT");  // 12&10 = 8
    assert_eq!(r.stack, vec![Value::Int(8)]);
    let r = run_src("PUSH 12\nPUSH 10\nBITOR\nHALT");   // 12|10 = 14
    assert_eq!(r.stack, vec![Value::Int(14)]);
    let r = run_src("PUSH 12\nPUSH 10\nBITXOR\nHALT");  // 12^10 = 6
    assert_eq!(r.stack, vec![Value::Int(6)]);
}

#[test]
fn bitwise_not_shift() {
    let r = run_src("PUSH 0\nBITNOT\nHALT");
    assert_eq!(r.stack, vec![Value::Int(-1)]);
    let r = run_src("PUSH 1\nPUSH 4\nSHL\nHALT");  // 1<<4 = 16
    assert_eq!(r.stack, vec![Value::Int(16)]);
    let r = run_src("PUSH 16\nPUSH 2\nSHR\nHALT");  // 16>>2 = 4
    assert_eq!(r.stack, vec![Value::Int(4)]);
}

// ── native string ops ──────────────────────────────────────────────────────────

#[test]
fn str_contains_native() {
    let r = run_src(r#"PUSH_STR "hello world"
    PUSH_STR "world"
    STR_CONTAINS
    HALT"#);
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src(r#"PUSH_STR "hello"
    PUSH_STR "xyz"
    STR_CONTAINS
    HALT"#);
    assert_eq!(r.stack, vec![Value::Bool(false)]);
}

#[test]
fn str_split_native() {
    let r = run_src(r#"PUSH_STR "a,b,c"
    PUSH_STR ","
    STR_SPLIT
    HALT"#);
    if let Some(Value::Array(arr)) = r.stack.first() {
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0], Value::Str("a".to_string()));
        assert_eq!(arr[1], Value::Str("b".to_string()));
        assert_eq!(arr[2], Value::Str("c".to_string()));
    } else {
        panic!("expected array");
    }
}

#[test]
fn str_replace_native() {
    let r = run_src(r#"PUSH_STR "hello world"
    PUSH_STR "world"
    PUSH_STR "there"
    STR_REPLACE
    HALT"#);
    assert_eq!(r.stack, vec![Value::Str("hello there".to_string())]);
}

#[test]
fn str_join_native() {
    let r = run_src(r#"PUSH_STR "a"
    PUSH_STR "b"
    PUSH_STR "c"
    NEW_ARRAY 3
    PUSH_STR ","
    STR_JOIN
    HALT"#);
    assert_eq!(r.stack, vec![Value::Str("a,b,c".to_string())]);
}

#[test]
fn make_range_native() {
    let r = run_src("PUSH 0\nPUSH 5\nMAKE_RANGE\nHALT");
    if let Some(Value::Array(arr)) = r.stack.first() {
        assert_eq!(arr.len(), 5);
        assert_eq!(arr[0], Value::Int(0));
        assert_eq!(arr[4], Value::Int(4));
    } else {
        panic!("expected array");
    }
    let r = run_src("PUSH 5\nPUSH 3\nMAKE_RANGE\nHALT");  // empty range
    assert_eq!(r.stack, vec![Value::Array(vec![])]);
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
        assert_eq!(arr.len(), 3);
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
        assert_eq!(arr.len(), 5);
    } else {
        panic!("expected array");
    }
}

fn map_is_truthy_only_when_non_empty() {
    assert!(!Value::Map(std::collections::HashMap::new()).is_truthy());
    let mut m = std::collections::HashMap::new();
    m.insert("x".to_string(), Value::Int(1));
    assert!(Value::Map(m).is_truthy());
}
