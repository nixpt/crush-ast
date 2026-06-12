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
fn float_push() {
    let r = run_src("PUSH_F64 3.14\nHALT");
    assert!(matches!(r.stack.first(), Some(Value::Float(f)) if (f - 3.14).abs() < 1e-10));
}

#[test]
fn comparisons() {
    let r = run_src("PUSH 3\nPUSH 5\nLT\nHALT");
    assert_eq!(r.stack, vec![Value::Int(1)]);
    let r = run_src("PUSH 5\nPUSH 3\nGT\nHALT");
    assert_eq!(r.stack, vec![Value::Int(1)]);
    let r = run_src("PUSH 5\nPUSH 5\nEQ\nHALT");
    assert_eq!(r.stack, vec![Value::Int(1)]);
}

#[test]
fn logical_not() {
    let r = run_src("PUSH 0\nNOT\nHALT");
    assert_eq!(r.stack, vec![Value::Int(1)]);
    let r = run_src("PUSH 42\nNOT\nHALT");
    assert_eq!(r.stack, vec![Value::Int(0)]);
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
    assert_eq!(r.stack, vec![Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)])]);
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
    let r = run_src("PUSH 10\nPUSH 20\nNEW_ARRAY 2\nPUSH 0\nPUSH 99\nARR_SET\nPUSH 0\nARR_GET\nHALT");
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
    let quotas = Quotas { max_steps: 10, ..Default::default() };
    assert!(run(&prog, &quotas).is_err());
}
