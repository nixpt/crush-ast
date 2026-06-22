use crate::assembler::{assemble, disassemble};
use crate::vm::{Quotas, Value, run};

/// Inlined canonical Crush-text → Value parser for the matrix test.
/// Mirrors `crush_lang_sdk::caps::text_as_value`: every input falls
/// into one of the recognised forms; unrecognised content falls
/// through to `Value::Str(s)`. **Inlined here, not imported** — a
/// `crush-lang-sdk` dev-dep on `crush-vm` would resolve through a
/// Cargo workspace cycle that links two distinct `crush_vm` instances
/// into the test binary (so `crush_lang_sdk::Value ≠ crush_vm::vm::Value`),
/// breaking the comparison. If the canonical parser in caps.rs ever
/// drifts, this inlined copy must be updated in lockstep. Depth-cap
/// from the original is omitted here because the matrix only feeds
/// well-formed Display output.
// Returns `Value` (not `Option<Value>`), matching the canonical
// `crush_lang_sdk::caps::text_as_value`.
fn parse_crush_text(s: &str) -> Value {
    if s == "null" {
        return Value::Null;
    }
    if s == "true" {
        return Value::Bool(true);
    }
    if s == "false" {
        return Value::Bool(false);
    }
    // Int: must precede Float — e.g. "3.0" fails i64::parse, so this
    // branch is safe; negative integers parse here cleanly.
    if let Ok(i) = s.parse::<i64>() {
        return Value::Int(i);
    }
    // Float: locks the `Display::{:.1}` form (e.g. "3.0" → 3.0_f64).
    if let Ok(f) = s.parse::<f64>() {
        return Value::Float(f);
    }

    let s_trim = s.trim();

    // Value::Array inverse — `[e1, e2, ...]` (comma-space joined on Display).
    if s_trim.starts_with('[') && s_trim.ends_with(']') {
        let inner = s_trim[1..s_trim.len() - 1].trim();
        if inner.is_empty() {
            return Value::new_array(vec![]);
        }
        let parsed = split_top_level_inline(inner, ',')
            .into_iter()
            .map(|p| parse_crush_text(p.trim()))
            .collect();
        return Value::new_array(parsed);
    }

    // Value::Map inverse — `{k: v, k2: v2}` (colon-space, comma-space joined).
    // Mirrors canonical `text_as_value::parse_value` exactly, including
    // the malformed-entry panic-to-Str contract: any pair without a
    // top-level `:` degenerates to `Value::Str(s)` (the whole input, not
    // inner/pair) so the parser cannot accidentally reconstruct a partial
    // map.
    if s_trim.starts_with('{') && s_trim.ends_with('}') {
        let inner = s_trim[1..s_trim.len() - 1].trim();
        if inner.is_empty() {
            return Value::new_map(std::collections::HashMap::new());
        }
        let mut m = std::collections::HashMap::new();
        for pair in split_top_level_inline(inner, ',') {
            if let Some((k, v)) = split_first_top_level_inline(pair.trim(), ':') {
                m.insert(k.trim().to_string(), parse_crush_text(v.trim()));
            } else {
                // Mirror canonical: malformed entry → identity Str(s).
                return Value::Str(s.to_string());
            }
        }
        return Value::new_map(m);
    }

    // Tagged-prefix forms (matches `text_as_value` precedence:
    // `error(msg)` first, then `<N bytes>`, then `<handle N>`,
    // finally Str fallback).
    if s.starts_with("error(") && s.ends_with(')') {
        return Value::Error(s[6..s.len() - 1].to_string());
    }
    if s.starts_with('<') && s.ends_with(" bytes>") {
        if let Ok(n) = s[1..s.len() - 7].parse::<usize>() {
            return Value::Bytes(vec![0; n]);
        }
    }
    if s.starts_with("<handle ") && s.ends_with('>') {
        if let Ok(id) = s[8..s.len() - 1].parse::<u64>() {
            return Value::Handle(id);
        }
    }

    Value::Str(s.to_string())
}

/// Top-level-aware comma separator (matches `text_as_value::split_top_level`).
fn split_top_level_inline(s: &str, delim: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut bd: i32 = 0;
    let mut brd: i32 = 0;
    let mut pd: i32 = 0;
    let mut start = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '[' => bd += 1,
            ']' => bd -= 1,
            '{' => brd += 1,
            '}' => brd -= 1,
            '(' => pd += 1,
            ')' => pd -= 1,
            _ if c == delim && bd == 0 && brd == 0 && pd == 0 => {
                parts.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

/// Top-level-aware first-occurrence separator (matches
/// `text_as_value::split_first_top_level`). Used to peel Map entry
/// `(key, value)` halves at the first top-level `:`.
fn split_first_top_level_inline(s: &str, delim: char) -> Option<(&str, &str)> {
    let mut bd: i32 = 0;
    let mut brd: i32 = 0;
    let mut pd: i32 = 0;
    for (i, c) in s.char_indices() {
        match c {
            '[' => bd += 1,
            ']' => bd -= 1,
            '{' => brd += 1,
            '}' => brd -= 1,
            '(' => pd += 1,
            ')' => pd -= 1,
            _ if c == delim && bd == 0 && brd == 0 && pd == 0 => {
                return Some((&s[..i], &s[i + c.len_utf8()..]));
            }
            _ => {}
        }
    }
    None
}

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
        vec![Value::new_array(vec![
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
        assert_eq!(arr.borrow().len(), 2);
        assert_eq!(arr.borrow()[0], Value::Int(1));
        assert_eq!(arr.borrow()[1], Value::Int(2));
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
        assert_eq!(arr.borrow().len(), 1);
        assert_eq!(arr.borrow()[0], Value::Int(1));
    } else {
        panic!("expected Array, got {:?}", r.stack[len - 2]);
    }
}

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
        assert_eq!(arr.borrow().len(), 3);
        assert_eq!(arr.borrow()[0], Value::Str("a".to_string()));
        assert_eq!(arr.borrow()[1], Value::Str("b".to_string()));
        assert_eq!(arr.borrow()[2], Value::Str("c".to_string()));
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
        assert_eq!(arr.borrow().len(), 5);
        assert_eq!(arr.borrow()[0], Value::Int(0));
        assert_eq!(arr.borrow()[4], Value::Int(4));
    } else {
        panic!("expected array");
    }
    let r = run_src("PUSH 5\nPUSH 3\nMAKE_RANGE\nHALT");  // empty range
    assert_eq!(r.stack, vec![Value::new_array(vec![])]);
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

fn map_is_truthy_only_when_non_empty() {
    assert!(!Value::new_map(std::collections::HashMap::new()).is_truthy());
    let mut m = std::collections::HashMap::new();
    m.insert("x".to_string(), Value::Int(1));
    assert!(Value::new_map(m).is_truthy());
}

// ── async / green threads ─────────────────────────────────────────────────────

#[test]
fn spawn_creates_handle() {
    // SPAWN with a function name should push a Handle, not null
    let src = "\
.func main
    PUSH_STR \"other\"
    SPAWN
    HALT
.func other
    HALT";
    let r = run_src(src);
    let top = r.stack.last().expect("should have a value");
    assert!(matches!(top, Value::Handle(_)), "expected Handle, got {top:?}");
}

#[test]
fn spawn_await_roundtrip() {
    // SPAWN a function, AWAIT it, check result
    let src = "\
.func main
    PUSH_STR \"worker\"
    SPAWN
    AWAIT
    HALT
.func worker
    PUSH 42
    HALT";
    let r = run_src(src);
    assert_eq!(r.stack, vec![Value::Int(42)]);
}

#[test]
fn yield_does_not_crash() {
    let r = run_src("PUSH 1\nYIELD\nPUSH 2\nHALT");
    assert_eq!(r.stack, vec![Value::Int(1), Value::Int(2)]);
}

// ── Combined round-trip matrix (single source-of-truth for the canonical ────
// ── trait triplet: Display / Serialize / Deserialize / text_as_value)     ────

#[test]
fn all_traits_round_trip_for_every_variant() {
    // Single source-of-truth matrix for the canonical trait triplet on
    // `crush_vm::vm::Value`:
    //
    //   `impl Display`           (line-rendering — canonical Crush text)
    //   `impl serde::Serialize`  (JSON wire-format)
    //   `impl serde::Deserialize` (canonical inverse of Serialize,
    //     including the tagged-form `<handle N>` / `<N bytes>` /
    //     `error(msg)` precedence in `visit_str`)
    //   `text_as_value`           (canonical Crush-text → Value parser;
    //     lives in `crush-lang-sdk::caps`, exercised here via the
    //     `crush-lang-sdk` dev-dep added to `crush-vm/Cargo.toml`).
    //
    // For each variant, four invariants are asserted under one matrix:
    //
    //   1. Display output is the canonical Crush text form —
    //      non-empty for every non-Null variant; explicit `"null"`
    //      for Null. (Sanity — no regressions that emit `""` for a
    //      Null, no regressions that emit `"(true)"` for a Bool, etc.)
    //
    //   2. `text_as_value ∘ Display == id` (i.e., the canonical Crush
    //      text form parses back to v).
    //
    //   3. `Deserialize ∘ Serialize == id` (i.e., the canonical JSON
    //      wire-format parses back to v).
    //
    //   4. For tagged forms (`Error`, `Bytes`, `Handle`), the
    //      inner-text segment produced by Serialize (the substring
    //      within the JSON string literal) equals what Display
    //      produces for the same variant — confirming the
    //      `<handle N>` / `<N bytes>` / `error(msg)` lockstep across
    //      the two formatters.
    //
    // Replaces the following 6 redundant tests with one matrix:
    //   - `display_map_renders_null_and_float_canonically`
    //   - `display_empty_map_renders_as_two_braces`
    //   - `serialize_produces_canonical_json_for_every_variant`
    //   - `deserialize_is_serialize_inverse_for_every_variant`
    //   - `deserialize_recognises_tagged_forms_by_prefix`
    //   - `text_as_value_is_display_inverse_for_every_variant` (caps.rs)
    //
    // Tests NOT removed by this matrix (kept because they exercise
    // distinctive concerns not covered here):
    //   - `caps::tests::text_as_value_edge_cases` — bracket-only inputs
    //     (`[[1,2],[3]]`, `{k:{nested:1}}`, `{key:null}`, `{k:error(oops)}`)
    //     that go through `text_as_value` path directly (no Display
    //     round-trip), exercising top-level-aware `split_*` helpers
    //     and the canonical Error-tagged nesting
    //   - `test_conv_to_str` / `test_json_parse` / `test_json_stringify`
    //     etc. (cap-gated integration tests in stdlib.rs + caps.rs)
    //   - the pre-fix `arr.len()` → `arr.borrow().len()` breadcrumb
    //     locks (one-liner structural regressions)

    let mut nested_inner = std::collections::HashMap::<String, Value>::new();
    nested_inner.insert("nested".to_string(), Value::Int(1));
    let mut nested_outer = std::collections::HashMap::<String, Value>::new();
    nested_outer.insert("outer".to_string(), Value::new_map(nested_inner));
    nested_outer.insert("sibling".to_string(), Value::Bool(true));

    let variants: Vec<(&str, Value)> = vec![
        ("Null", Value::Null),
        ("Bool true", Value::Bool(true)),
        ("Bool false", Value::Bool(false)),
        ("Int -7", Value::Int(-7)),
        ("Int 0 (sign-edge)", Value::Int(0)),
        ("Float 3.14 (fractional)", Value::Float(3.14)),
        ("Float 0.0 (zero-edge)", Value::Float(0.0)),
        ("Float 3.0 (.0 suffix lockstep)", Value::Float(3.0)),
        ("Str foo", Value::Str("foo".to_string())),
        ("Str with escapes", Value::Str(r#"a"b\c"#.to_string())),
        (
            "Bytes 3 (length-only caveat, zero-fill)",
            Value::Bytes(vec![0, 0, 0]),
        ),
        ("Handle 42 (tagged-form lockstep)", Value::Handle(42)),
        ("Error oops (tagged-form lockstep)", Value::Error("oops".to_string())),
        (
            "Array [1, 2] (single-level)",
            Value::new_array(vec![Value::Int(1), Value::Int(2)]),
        ),
        (
            "Array nested (locks visit_seq recursion)",
            Value::new_array(vec![
                Value::new_array(vec![Value::Int(1), Value::Int(2)]),
                Value::new_array(vec![Value::Int(3)]),
            ]),
        ),
        ("Array empty (edge case)", Value::new_array(vec![])),
        (
            "Map {k:42, k2:\"v\"} (single-level)",
            Value::new_map({
                let mut m = std::collections::HashMap::<String, Value>::new();
                m.insert("k".to_string(), Value::Int(42));
                m.insert("k2".to_string(), Value::Str("v".to_string()));
                m
            }),
        ),
        ("Map nested (locks visit_map recursion)", Value::new_map(nested_outer)),
        ("Map empty (edge case)", Value::new_map(std::collections::HashMap::<String, Value>::new())),
    ];

    for (label, v) in variants {
        // Invariant 1: Display produces canonical Crush text form.
        let display_str = v.to_string();
        match &v {
            Value::Null => assert_eq!(
                display_str, "null",
                "{label}: Display should be 'null' for Value::Null, got {display_str:?}"
            ),
            _ => assert!(
                !display_str.is_empty(),
                "{label}: Display output should be non-empty, got {display_str:?}"
            ),
        }

        // Invariant 2: text_as_value ∘ Display == identity.
        let parsed_via_display = parse_crush_text(&display_str);
        assert_eq!(
            parsed_via_display, v,
            "{label}: text_as_value(Display(v)) != v; Display was {display_str:?}, parsed={parsed_via_display:?}"
        );

        // Invariant 3: Deserialize ∘ Serialize == identity.
        let json_str = serde_json::to_string(&v)
            .unwrap_or_else(|e| panic!("{label}: Serialize failed: {e}"));
        let parsed_via_json: Value = serde_json::from_str(&json_str)
            .unwrap_or_else(|e| panic!("{label}: Deserialize failed for {json_str:?}: {e}"));
        assert_eq!(
            parsed_via_json, v,
            "{label}: Deserialize(Serialize(v)) != v; Serialize was {json_str:?}, parsed={parsed_via_json:?}"
        );

        // Invariant 4: For tagged forms, lockstep between Display text
        // form and the inner-text segment of Serialize's JSON output.
        // The Serialize-text inner segment is the JSON-string body of
        // the JSON-quoted form; e.g. Display emits "<handle 42>" and
        // Serialize emits "\"<handle 42>\"" (with surrounding JSON
        // quotes), so the inner segment is identical after stripping.
        match &v {
            Value::Error(e) => {
                let expected_inner = format!("error({e})");
                assert_eq!(
                    display_str, expected_inner,
                    "{label}: Display text should be {expected_inner:?}, got {display_str:?}"
                );
                let expected_json = format!("\"{expected_inner}\"");
                assert_eq!(
                    json_str, expected_json,
                    "{label}: Serialize JSON should be {expected_json:?}, got {json_str:?}"
                );
            }
            Value::Bytes(b) => {
                let expected_inner = format!("<{} bytes>", b.len());
                assert_eq!(
                    display_str, expected_inner,
                    "{label}: Display text should be {expected_inner:?}, got {display_str:?}"
                );
                let expected_json = format!("\"{expected_inner}\"");
                assert_eq!(
                    json_str, expected_json,
                    "{label}: Serialize JSON should be {expected_json:?}, got {json_str:?}"
                );
            }
            Value::Handle(id) => {
                let expected_inner = format!("<handle {id}>");
                assert_eq!(
                    display_str, expected_inner,
                    "{label}: Display text should be {expected_inner:?}, got {display_str:?}"
                );
                let expected_json = format!("\"{expected_inner}\"");
                assert_eq!(
                    json_str, expected_json,
                    "{label}: Serialize JSON should be {expected_json:?}, got {json_str:?}"
                );
            }
            _ => {}
        }
    }
}

    #[test]
    fn test_json_parse_bytes_lossy_round_trip_inline() {
        // **Trait-layer lock for the `<N bytes>` lossy round-trip**:
        // The canonical `impl Serialize for Value::Bytes(b)` emits
        // ONLY the length-prefix inner-content `<{N} bytes>` (e.g.
        // `<3 bytes>` for `vec![1,2,3]`); actual byte contents are
        // NOT preserved through the JSON wire. `serde_json::to_string`
        // wraps that inner tag in surrounding JSON quotes before
        // returning the 11-char Rust String `r#""<3 bytes>""#`.
        // Re-parsing the recovered JSON-quoted tag via canonical
        // `impl Deserialize for Value::visit_str` reconstructs a
        // ZERO-FILLED `Vec<u8>` of length N — NOT the original
        // byte payload.
        //
        // This TRAIT-LAYER test pins the lossiness contract
        // end-to-end through the canonical `serde` trait impls
        // (NOT through the `json.parse`/`json.stringify` cap layer,
        // which is locked separately in
        // `crush-lang-sdk::tests::test_json_parse_tagged_forms::
        // fixture 6`). Drift in either trait impl would surface
        // here as an `assert_eq!` mismatch, NOT silently pass
        // through either path layer.

        let bytes_value = Value::Bytes(vec![1u8, 2, 3]);

        // Step A: `serde_json::to_string(&Value::Bytes(vec![1,2,3]))`
        // emits the JSON-quoted length-only tag `r#""<3 bytes>""#` —
        // byte CONTENTS dropped, length preserved. The trait impl
        // emits the bare inner tag `<3 bytes>` (9 chars); serde_json
        // wraps it in surrounding `"`s before returning the 11-char
        // String.
        let serialized_json = serde_json::to_string(&bytes_value)
            .expect("Serialize for Value::Bytes should not fail");
        assert_eq!(
            serialized_json, r#""<3 bytes>""#,
            "canonical Serialize for Value::Bytes(vec![1,2,3]) at the trait layer \
             should emit the JSON-quoted length-only tag \"<3 bytes>\" (byte \
             contents intentionally stripped), got {serialized_json:?}"
        );

        // Step B: `serde_json::from_str::<Value>(&"\"<3 bytes>\"")`
        // reconstructs a ZERO-FILLED Vec<u8> of length N — NOT the
        // original `vec![1, 2, 3]` payload. Documented length-only
        // caveat; byte preservation through JSON wire format is
        // NOT a goal.
        let parsed: Value = serde_json::from_str(&serialized_json)
            .expect("Deserialize for \"<3 bytes>\" should not fail");
        match parsed {
            Value::Bytes(reconstructed) => assert_eq!(
                reconstructed, vec![0u8, 0, 0],
                "LOSSY ROUND-TRIP: canonical Deserialize for \"<3 bytes>\" \
                 reconstructs a ZERO-FILLED Vec<u8> of length N (NOT the \
                 original byte payload vec![1,2,3]). Got {:?}, expected \
                 vec![0,0,0].",
                reconstructed
            ),
            other => panic!(
                "FAIL: canonical Deserialize for \"<3 bytes>\" should produce \
                 Value::Bytes(vec![0,0,0]) (zero-filled per the length-only \
                 caveat), got {other:?}"
            ),
        }        // No Step C: Steps A+B jointly prove `parsed != bytes_value` (Step A
        // pins Serialize's exact `<3 bytes>` form, Step B pins
        // Deserialize's exact `vec![0,0,0]` reconstruction). An
        // `assert_ne!` here would also move-conflict with Step B's
        // `Value::Bytes(reconstructed)` binding.
    }

    // ── cross-parser matrix ────────────────────────────────────────
    //
    // Locks the JSON-text-vs-Crush-text inverse parallelism for the
    // FOUR boundary fixtures that historically are the parser-drift
    // surface. Each fixture is fed through BOTH the Crush-text path
    // (the inlined `parse_crush_text` mirror of canonical
    // `caps::parse_value` — see its docstring for the Cargo-cycle
    // rationale that prevents direct `caps::parse_value` invocation
    // from `crush-vm::tests`) AND the JSON path (canonical
    // `impl Deserialize for Value::visit_str`, exercised via
    // `serde_json::from_str::<Value>(&serde_json::to_string(
    // &serde_json::Value::String(content.to_string()))?)`). The
    // third assertion on each fixture is the cross-parity lock —
    // if ONE side drifts from the other, the panic names the
    // affected fixture and which side produced the divergent value.
    //
    // Companion to `all_traits_round_trip_for_every_variant` (which
    // locks `text_as_value ∘ Display == id` and `Deserialize ∘
    // Serialize == id` separately for every variant). THIS test
    // adds the linkage: text-path output === JSON-path output for
    // the SAME canonical content at the parser-drift boundary.
    // Without this linkage, `caps::parse_value` could drift from
    // `impl Deserialize::visit_str` (or vice-versa) silently — a
    // `json.parse("error((foo)")` could land on one canonical form
    // while `crush -e 'error((foo)'` (text path) lands on another.
    //
    // Drift sources caught:
    //  • `impl Deserialize::visit_str` → `accept ! (1)` fails.
    //  • `caps::parse_value` (via mirror drift) → `accept ! (1)`
    //    fails; reader compared `from_json` to canonical-expected
    //    and panic names JSON-side first (canonical) before text.
    //  • `Value::Display` for the tagged forms → on Display round-
    //    trip, neither path would reach the boundary fixture's
    //    expected output; this also catches that drift.
    #[test]
    fn test_text_vs_json_inverse_parser_matrix() {
        // (canonical_content, expected_value_after_parse)
        //
        // Each entry is parsed via `parse_crush_text(content)`
        // (the inlined mirror of canonical `caps::text_as_value`)
        // AND `serde_json::from_str::<Value>(&serde_json::to_string(
        // &serde_json::Value::String(content.to_string())).unwrap())`
        // — JSON-quoting via the canonical `serde_json::Value::String`
        // pipeline matches the wire-form the cap layer
        // (`json.stringify` → `json.parse`) produces end-to-end.
        let fixtures: &[(&str, Value)] = &[
            // Boundary 1: `<handle N>` tagged form. Both paths
            // extract the integer `N` from inside the brackets.
            ("<handle 42>", Value::Handle(42)),
            // Boundary 2: `<N bytes>` length-tag. Both paths
            // reconstruct a zero-filled `Vec<u8>` of length N
            // (documented length-only caveat — actual byte payload
            // is NOT preserved through either path).
            ("<3 bytes>", Value::Bytes(vec![0u8, 0, 0])),
            // Boundary 3: `error((foo)` nested-open. The
            // `s[6..s.len() - 1]` slice formula strips ONE leading
            // wrap and ONE trailing `)`, preserving the
            // inner-most opening paren. NOT a balanced-paren walk.
            (
                "error((foo)",
                Value::Error("(foo".to_string()),
            ),
            // Boundary 4: `error(foo))` nested-close. Same slice,
            // preserves the inner-most closing paren.
            (
                "error(foo))",
                Value::Error("foo)".to_string()),
            ),
        ];

        for &(content, ref expected) in fixtures {
            // Crush-text path: direct canonical content via the
            // inlined mirror of `caps::text_as_value`.
            let from_text = parse_crush_text(content);
            assert_eq!(
                from_text, *expected,
                "TEXT-side drift on fixture {:?}: parse_crush_text({:?}) \
                 produced {:?}, expected {:?}",
                content, content, from_text, *expected
            );

            // JSON path: route through canonical
            // `serde_json::Value::String(content).to_string()` to
            // produce a JSON-quoted envelope, then parse back
            // through canonical `impl Deserialize for Value`. This
            // mirrors the wire-form the cap layer produces.
            let json_quoted = serde_json::to_string(
                &serde_json::Value::String(content.to_string()),
            )
            .expect("serde_json::Value::String always serializes");
            let from_json: Value = serde_json::from_str(&json_quoted)
                .expect("JSON-quoted canonical content always parses");
            assert_eq!(
                from_json, *expected,
                "JSON-side drift on fixture {:?}: \
                 serde_json::from_str::<Value>({}) produced {:?}, \
                 expected {:?}",
                content, json_quoted, from_json, *expected
            );

            // CROSS-PARSER PARITY: text-path output MUST equal
            // JSON-path output for the same canonical content. If
            // ONE side drifts from the other, this assertion
            // pinpoints which side diverged. The panic message
            // also dumps `*expected` so a future debugger can
            // identify the drifter BY INSPECTION: the side
            // (text vs json) that differs from `expected` is the
            // one that drifted. This is the regression lock that
            // the user's audit flagged as missing — without
            // this assertion, `caps::parse_value` and
            // `impl Deserialize::visit_str` could drift
            // independently without CI catching it.
            assert_eq!(
                from_text, from_json,
                "CROSS-PARSER DRIFT on canonical content {:?}: \
                 text-path={:?}, JSON-path={:?}, expected={:?}. \
                 Either `caps::parse_value` (and its test-mirror \
                 `parse_crush_text`) OR `impl Deserialize::visit_str` \
                 drifted from each other; by inspection, the side \
                 that differs from `expected` is the drifter. \
                 Companion matrices: see the doc-comment on this test \
                 function and on `parse_crush_text`.",
                content, from_text, from_json, *expected,
            );
        }
    }
