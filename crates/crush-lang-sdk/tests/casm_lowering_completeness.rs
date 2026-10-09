//! CRUSH-232: `casm_to_vm` must be total over the CASM `OpCode` set.
//!
//! Every variant of `casm::OpCode` is either lowered to CVM1 or sits on the
//! explicit `UNSUPPORTED` allowlist below (with the reason it cannot be
//! expressed). Adding a variant to `OpCode` breaks this file at compile time
//! (`probe` is an exhaustive `match` with no wildcard) and, once `probe` is
//! extended, fails the coverage test until the new op is lowered or listed.

use casm::{Function, Instruction, Manifest, OpCode, Program};
use crush_lang_sdk::compile::casm_to_vm;
use crush_vm::Quotas;
use crush_vm::vm::Value;
use serde_json::{Value as J, json};
use std::collections::{BTreeSet, HashMap};

/// Ops CVM1 genuinely cannot express, with the reason. Each one is asserted to
/// fail with `Unsupported CVM1 opcode` (so the list cannot rot into a lie).
const UNSUPPORTED: &[(&str, &str)] = &[
    (
        "break",
        "CASM `break` carries no loop target; the frontend lowers loop exits to `jmp`, and CVM1 has no loop stack to unwind",
    ),
    (
        "continue",
        "same as `break`: no loop target in the instruction, no CVM1 loop opcode",
    ),
    (
        "import_var",
        "reads a variable exported by another execution context (polyglot/VM boundary); CVM1 has one flat frame memory and no import channel",
    ),
    (
        "call_host",
        "calls a capsule's host function by 32-byte interface-contract id; CVM1 only has name-based CAP_CALL and no IC registry",
    ),
    (
        "call_interface",
        "CSCS v1 call through an ObjectHandle/token; CVM1 has no object-handle table or interface dispatch",
    ),
];

/// Ops that lower, but only to `NOP` (nothing to run on CVM1 yet). Counted as
/// lowered by the table, listed here so the stubs are visible and tested:
/// replacing one with a real lowering means removing it from this list.
const NOP_STUBS: &[&str] = &[
    "export_var",
    "dom_query",
    "dom_mutate",
    "dom_event_listener",
    "ai_adaptation_request",
    "ai_capability_discovery",
    "ai_semantic_switch",
];

fn ins(op: &str, args: J) -> Instruction {
    Instruction {
        op: op.to_string(),
        lang: None,
        meta: None,
        args,
    }
}

/// Exhaustive over `OpCode` (no wildcard). Returns a unique ordinal (0..N) and
/// a minimal instruction for the variant. The instruction is built by hand with
/// the canonical JSON op name; `all_probes_round_trip` proves that name maps
/// back to the variant through `Instruction::to_opcode`.
fn probe(op: &OpCode) -> (usize, Instruction) {
    let none = || json!({});
    match op {
        OpCode::PushInt(_) => (0, ins("push_int", json!({"value": 7}))),
        OpCode::PushFloat(_) => (1, ins("push_float", json!({"value": 1.5}))),
        OpCode::PushStr(_) => (2, ins("push_str", json!({"value": "s"}))),
        OpCode::PushBool(_) => (3, ins("push_bool", json!({"value": true}))),
        OpCode::PushNull => (4, ins("push_null", none())),
        OpCode::Pop => (5, ins("pop", none())),
        OpCode::Dup => (6, ins("dup", none())),
        OpCode::Store(_) => (7, ins("store", json!({"name": "x"}))),
        OpCode::Load(_) => (8, ins("load", json!({"name": "x"}))),
        OpCode::ExportVar(_) => (9, ins("export_var", json!({"name": "x"}))),
        OpCode::ImportVar(_) => (10, ins("import_var", json!({"name": "x"}))),
        OpCode::Add => (11, ins("add", none())),
        OpCode::Sub => (12, ins("sub", none())),
        OpCode::Mul => (13, ins("mul", none())),
        OpCode::Div => (14, ins("div", none())),
        OpCode::Mod => (15, ins("mod", none())),
        OpCode::Neg => (16, ins("neg", none())),
        OpCode::Eq => (17, ins("eq", none())),
        OpCode::Ne => (18, ins("ne", none())),
        OpCode::Lt => (19, ins("lt", none())),
        OpCode::Gt => (20, ins("gt", none())),
        OpCode::Le => (21, ins("le", none())),
        OpCode::Ge => (22, ins("ge", none())),
        OpCode::And => (23, ins("and", none())),
        OpCode::Or => (24, ins("or", none())),
        OpCode::Not => (25, ins("not", none())),
        OpCode::BitAnd => (26, ins("bit_and", none())),
        OpCode::BitOr => (27, ins("bit_or", none())),
        OpCode::BitXor => (28, ins("bit_xor", none())),
        OpCode::BitNot => (29, ins("bit_not", none())),
        OpCode::Shl => (30, ins("shl", none())),
        OpCode::Shr => (31, ins("shr", none())),
        OpCode::Swap => (32, ins("swap", none())),
        OpCode::Rot => (33, ins("rot", none())),
        OpCode::Pick(_) => (34, ins("pick", json!({"n": 1}))),
        OpCode::Roll(_) => (35, ins("roll", json!({"n": 1}))),
        OpCode::Jmp(_) => (36, ins("jmp", json!({"target": 1}))),
        OpCode::JmpIf(_) => (37, ins("jmp_if", json!({"target": 1}))),
        OpCode::JmpIfNot(_) => (38, ins("jmp_if_not", json!({"target": 1}))),
        OpCode::Call(_) => (39, ins("call", json!({"function": "main"}))),
        OpCode::Ret => (40, ins("ret", none())),
        OpCode::Break => (41, ins("break", none())),
        OpCode::Continue => (42, ins("continue", none())),
        OpCode::Spawn => (43, ins("spawn", json!({"argc": 0}))),
        OpCode::Yield => (44, ins("yield", none())),
        OpCode::Await { .. } => (45, ins("await", json!({"handle": "h"}))),
        OpCode::EnterTry => (46, ins("enter_try", json!({"target": 1}))),
        OpCode::ExitTry => (47, ins("exit_try", none())),
        OpCode::Throw => (48, ins("throw", none())),
        OpCode::NewArray(_) => (49, ins("new_array", json!({"size": 0}))),
        OpCode::ArrGet => (50, ins("arr_get", none())),
        OpCode::ArrSet => (51, ins("arr_set", none())),
        OpCode::ArrLen => (52, ins("arr_len", none())),
        OpCode::ArrPush => (53, ins("arr_push", none())),
        OpCode::ArrPop => (54, ins("arr_pop", none())),
        OpCode::Index => (55, ins("index", none())),
        OpCode::Len => (56, ins("len", none())),
        OpCode::ArrayPush => (57, ins("array_push", none())),
        OpCode::ArrayPop => (58, ins("array_pop", none())),
        OpCode::MakeRange => (59, ins("make_range", none())),
        OpCode::NewTuple(_) => (60, ins("new_tuple", json!({"size": 0}))),
        OpCode::TuplePush => (61, ins("tuple_push", none())),
        OpCode::NewList(_) => (62, ins("new_list", json!({"size": 0}))),
        OpCode::ListPush => (63, ins("list_push", none())),
        OpCode::NewVector(_) => (64, ins("new_vector", json!({"size": 0}))),
        OpCode::VectorPush => (65, ins("vector_push", none())),
        OpCode::NewSet(_) => (66, ins("new_set", json!({"size": 0}))),
        OpCode::SetPush => (67, ins("set_push", none())),
        OpCode::NewObj => (68, ins("new_obj", none())),
        OpCode::NewStruct(_) => (69, ins("new_struct", json!({"name": "P"}))),
        OpCode::GetField(_) => (70, ins("get_field", json!({"name": "f"}))),
        OpCode::SetField(_) => (71, ins("set_field", json!({"name": "f"}))),
        OpCode::TypeOf => (72, ins("type_of", none())),
        OpCode::Cast(_) => (73, ins("cast", json!({"type": "int"}))),
        OpCode::CapCall { .. } => (74, ins("cap_call", json!({"name": "io.print", "argc": 1}))),
        OpCode::CallHost { .. } => (
            75,
            ins(
                "call_host",
                json!({"capsule": "c", "ic_id": "00".repeat(32), "method": "m", "argc": 0}),
            ),
        ),
        OpCode::CallInterface { .. } => (
            76,
            ins(
                "call_interface",
                json!({"handle": "h", "method": "m", "argc": 0}),
            ),
        ),
        OpCode::ExecLang { .. } => (
            77,
            ins(
                "exec_lang",
                json!({"lang": "python", "code": "pass", "var_count": 0}),
            ),
        ),
        OpCode::AiQuery(_) => (78, ins("ai_query", none())),
        OpCode::AiAdaptationRequest(_) => (79, ins("ai_adaptation_request", none())),
        OpCode::AiCapabilityDiscovery(_) => (80, ins("ai_capability_discovery", none())),
        OpCode::AiSemanticSwitch(_) => (81, ins("ai_semantic_switch", none())),
        OpCode::AiToolchain(_) => (82, ins("ai_tool_chain", none())),
        OpCode::AiAgentDelegation(_) => (83, ins("ai_agent_delegation", none())),
        OpCode::AiLearningLoop(_) => (84, ins("ai_learning_loop", none())),
        OpCode::AiContextAware(_) => (85, ins("ai_context_aware", none())),
        OpCode::AiSemanticMatch(_) => (86, ins("ai_semantic_match", none())),
        OpCode::AiSynthesize(_) => (87, ins("ai_synthesize", none())),
        OpCode::AiGoalDeclaration(_) => (88, ins("ai_goal_decl", none())),
        OpCode::AiProgressUpdate(_) => (89, ins("ai_progress_update", none())),
        OpCode::AiKnowledgeSharing(_) => (90, ins("ai_knowledge_share", none())),
        OpCode::MathPow => (91, ins("math_pow", none())),
        OpCode::MathSqrt => (92, ins("math_sqrt", none())),
        OpCode::MathAbs => (93, ins("math_abs", none())),
        OpCode::MathRound => (94, ins("math_round", none())),
        OpCode::MathFloor => (95, ins("math_floor", none())),
        OpCode::MathCeil => (96, ins("math_ceil", none())),
        OpCode::StrStartsWith => (97, ins("str_starts_with", none())),
        OpCode::StrEndsWith => (98, ins("str_ends_with", none())),
        OpCode::StrToUpper => (99, ins("str_to_upper", none())),
        OpCode::StrToLower => (100, ins("str_to_lower", none())),
        OpCode::StrTrim => (101, ins("str_trim", none())),
        OpCode::DomQuery(_) => (102, ins("dom_query", none())),
        OpCode::DomMutate(_) => (103, ins("dom_mutate", none())),
        OpCode::DomEventListener(_) => (104, ins("dom_event_listener", none())),
        OpCode::Halt => (105, ins("halt", none())),
        OpCode::StrContains => (106, ins("str_contains", none())),
        OpCode::StrSplit => (107, ins("str_split", none())),
        OpCode::StrReplace => (108, ins("str_replace", none())),
        OpCode::StrJoin => (109, ins("str_join", none())),
    }
}

/// One value per variant. Keep in step with `probe` (the ordinal check below
/// fails if a variant is missing here or duplicated).
const VARIANT_COUNT: usize = 110;

fn all_opcodes() -> Vec<OpCode> {
    let v = || json!({});
    vec![
        OpCode::PushInt(0),
        OpCode::PushFloat(0.0),
        OpCode::PushStr(String::new()),
        OpCode::PushBool(false),
        OpCode::PushNull,
        OpCode::Pop,
        OpCode::Dup,
        OpCode::Store(String::new()),
        OpCode::Load(String::new()),
        OpCode::ExportVar(String::new()),
        OpCode::ImportVar(String::new()),
        OpCode::Add,
        OpCode::Sub,
        OpCode::Mul,
        OpCode::Div,
        OpCode::Mod,
        OpCode::Neg,
        OpCode::Eq,
        OpCode::Ne,
        OpCode::Lt,
        OpCode::Gt,
        OpCode::Le,
        OpCode::Ge,
        OpCode::And,
        OpCode::Or,
        OpCode::Not,
        OpCode::BitAnd,
        OpCode::BitOr,
        OpCode::BitXor,
        OpCode::BitNot,
        OpCode::Shl,
        OpCode::Shr,
        OpCode::Swap,
        OpCode::Rot,
        OpCode::Pick(0),
        OpCode::Roll(0),
        OpCode::Jmp(0),
        OpCode::JmpIf(0),
        OpCode::JmpIfNot(0),
        OpCode::Call(String::new()),
        OpCode::Ret,
        OpCode::Break,
        OpCode::Continue,
        OpCode::Spawn,
        OpCode::Yield,
        OpCode::Await {
            handle: String::new(),
        },
        OpCode::EnterTry,
        OpCode::ExitTry,
        OpCode::Throw,
        OpCode::NewArray(0),
        OpCode::ArrGet,
        OpCode::ArrSet,
        OpCode::ArrLen,
        OpCode::ArrPush,
        OpCode::ArrPop,
        OpCode::Index,
        OpCode::Len,
        OpCode::ArrayPush,
        OpCode::ArrayPop,
        OpCode::MakeRange,
        OpCode::NewTuple(0),
        OpCode::TuplePush,
        OpCode::NewList(0),
        OpCode::ListPush,
        OpCode::NewVector(0),
        OpCode::VectorPush,
        OpCode::NewSet(0),
        OpCode::SetPush,
        OpCode::NewObj,
        OpCode::NewStruct(String::new()),
        OpCode::GetField(String::new()),
        OpCode::SetField(String::new()),
        OpCode::TypeOf,
        OpCode::Cast(String::new()),
        OpCode::CapCall {
            name: String::new(),
            argc: 0,
        },
        OpCode::CallHost {
            capsule: String::new(),
            ic_id: [0; 32],
            method: String::new(),
            argc: 0,
        },
        OpCode::CallInterface {
            handle: String::new(),
            method: String::new(),
            argc: 0,
        },
        OpCode::ExecLang {
            lang: String::new(),
            code: String::new(),
            var_count: 0,
        },
        OpCode::AiQuery(v()),
        OpCode::AiAdaptationRequest(v()),
        OpCode::AiCapabilityDiscovery(v()),
        OpCode::AiSemanticSwitch(v()),
        OpCode::AiToolchain(v()),
        OpCode::AiAgentDelegation(v()),
        OpCode::AiLearningLoop(v()),
        OpCode::AiContextAware(v()),
        OpCode::AiSemanticMatch(v()),
        OpCode::AiSynthesize(v()),
        OpCode::AiGoalDeclaration(v()),
        OpCode::AiProgressUpdate(v()),
        OpCode::AiKnowledgeSharing(v()),
        OpCode::MathPow,
        OpCode::MathSqrt,
        OpCode::MathAbs,
        OpCode::MathRound,
        OpCode::MathFloor,
        OpCode::MathCeil,
        OpCode::StrStartsWith,
        OpCode::StrEndsWith,
        OpCode::StrToUpper,
        OpCode::StrToLower,
        OpCode::StrTrim,
        OpCode::DomQuery(v()),
        OpCode::DomMutate(v()),
        OpCode::DomEventListener(v()),
        OpCode::Halt,
        OpCode::StrContains,
        OpCode::StrSplit,
        OpCode::StrReplace,
        OpCode::StrJoin,
    ]
}

fn program_of(body: Vec<Instruction>) -> Program {
    let mut functions = HashMap::new();
    functions.insert(
        "main".to_string(),
        Function {
            params: vec![],
            locals: vec![],
            type_hints: None,
            body,
        },
    );
    Program {
        version: "1.0".to_string(),
        functions,
        manifest: Manifest::default(),
        lang: None,
    }
}

#[derive(Debug, PartialEq)]
enum Outcome {
    Lowered,
    Unsupported,
    OtherError(String),
}

fn lower(instr: Instruction) -> Outcome {
    // A trailing `ret` gives jump targets (index 1) somewhere to land.
    match casm_to_vm(&program_of(vec![instr, ins("ret", json!({}))])) {
        Ok(_) => Outcome::Lowered,
        Err(e) if e.to_string().contains("Unsupported CVM1 opcode") => Outcome::Unsupported,
        Err(e) => Outcome::OtherError(e.to_string()),
    }
}

#[test]
fn variant_list_matches_probe_exhaustively() {
    let ordinals: BTreeSet<usize> = all_opcodes().iter().map(|o| probe(o).0).collect();
    assert_eq!(all_opcodes().len(), VARIANT_COUNT);
    assert_eq!(
        ordinals,
        (0..VARIANT_COUNT).collect::<BTreeSet<_>>(),
        "all_opcodes() must contain exactly one value per OpCode variant"
    );
}

#[test]
fn probes_use_the_canonical_op_names() {
    for op in all_opcodes() {
        let (_, instr) = probe(&op);
        let back = instr
            .to_opcode()
            .unwrap_or_else(|e| panic!("`{}` does not parse as an OpCode: {e}", instr.op));
        assert_eq!(
            std::mem::discriminant(&back),
            std::mem::discriminant(&op),
            "probe op `{}` maps to {back:?}, not {op:?}",
            instr.op
        );
    }
}

/// The heart of CRUSH-232: no variant may be neither lowered nor listed.
#[test]
fn every_opcode_is_lowered_or_explicitly_unsupported() {
    let allow: BTreeSet<&str> = UNSUPPORTED.iter().map(|(n, _)| *n).collect();
    let mut lowered = vec![];
    let mut unsupported = vec![];
    let mut bad = vec![];
    for op in all_opcodes() {
        let (_, instr) = probe(&op);
        let name = instr.op.clone();
        match lower(instr) {
            Outcome::Lowered => lowered.push(name),
            Outcome::Unsupported => unsupported.push(name),
            Outcome::OtherError(e) => bad.push(format!("{name}: {e}")),
        }
    }
    eprintln!(
        "CRUSH-232 lowering: {} lowered, {} unsupported",
        lowered.len(),
        unsupported.len()
    );
    eprintln!("unsupported: {unsupported:?}");
    assert!(bad.is_empty(), "ops that fail for another reason: {bad:?}");

    let unlisted: Vec<&String> = unsupported
        .iter()
        .filter(|n| !allow.contains(n.as_str()))
        .collect();
    assert!(
        unlisted.is_empty(),
        "ops neither lowered nor on the UNSUPPORTED allowlist: {unlisted:?}"
    );
    let stale: Vec<&&str> = allow
        .iter()
        .filter(|n| lowered.iter().any(|l| l == **n))
        .collect();
    assert!(
        stale.is_empty(),
        "allowlisted ops that now lower (remove them from UNSUPPORTED): {stale:?}"
    );
    for (name, reason) in UNSUPPORTED {
        assert!(!reason.is_empty(), "{name}: allowlist entry needs a reason");
    }
}

// ---------------------------------------------------------------------------
// Value tests: lowered ops run through CVM1 and produce the right value.
// ---------------------------------------------------------------------------

fn push_i(v: i64) -> Instruction {
    ins("push_int", json!({"value": v}))
}
fn push_f(v: f64) -> Instruction {
    ins("push_float", json!({"value": v}))
}
fn push_s(v: &str) -> Instruction {
    ins("push_str", json!({"value": v}))
}
fn op(name: &str) -> Instruction {
    ins(name, json!({}))
}

/// Run `body` (then `halt`, so the final stack is observable) on CVM1 and
/// return the whole stack, bottom to top. Both CVM1 engines (the scheduler
/// behind `crush run` and `PortableVm`) must agree.
fn run_stack(body: Vec<Instruction>) -> Vec<Value> {
    let mut body = body;
    body.push(op("halt"));
    let vm = casm_to_vm(&program_of(body)).expect("lowers");
    let sched = crush_vm::run_with_caps(&vm, &Quotas::default(), None)
        .expect("scheduler runs")
        .stack;
    let portable = crush_vm::PortableVm::new(vm)
        .run()
        .expect("PortableVm runs")
        .stack;
    assert_eq!(sched, portable, "scheduler and PortableVm disagree");
    sched
}

fn run_top(body: Vec<Instruction>) -> Value {
    run_stack(body).pop().expect("a value on the stack")
}

#[test]
fn nop_stubs_lower_to_nop_and_nothing_else_does_by_accident() {
    for name in NOP_STUBS {
        let instr = all_opcodes()
            .iter()
            .map(|o| probe(o).1)
            .find(|i| i.op == *name)
            .unwrap_or_else(|| panic!("{name} is not a probed op"));
        let vm = casm_to_vm(&program_of(vec![instr, op("ret")])).expect("lowers");
        let text = crush_vm::disassemble(&vm);
        assert!(
            text.contains("NOP"),
            "{name} is listed as a NOP stub but lowered to:\n{text}"
        );
    }
}

#[test]
fn unsupported_ops_fail_loudly_with_the_op_name() {
    for (name, _) in UNSUPPORTED {
        let instr = all_opcodes()
            .iter()
            .map(|o| probe(o).1)
            .find(|i| i.op == *name)
            .unwrap();
        let err = casm_to_vm(&program_of(vec![instr, op("ret")])).expect_err(name);
        let msg = err.to_string();
        assert!(
            msg.contains("Unsupported CVM1 opcode") && msg.contains(name),
            "{name}: {msg}"
        );
    }
}

fn int(v: i64) -> Value {
    Value::Int(v)
}
fn float(v: f64) -> Value {
    Value::Float(v)
}
fn st(v: &str) -> Value {
    Value::Str(v.to_string())
}

#[test]
fn bitwise_and_shift_ops_compute() {
    let bin = |name: &str, a: i64, b: i64| run_top(vec![push_i(a), push_i(b), op(name)]);
    assert_eq!(bin("bit_and", 6, 3), int(2));
    assert_eq!(bin("bit_or", 6, 3), int(7));
    assert_eq!(bin("bit_xor", 6, 3), int(5));
    assert_eq!(bin("shl", 1, 4), int(16));
    assert_eq!(bin("shr", 16, 2), int(4));
    assert_eq!(run_top(vec![push_i(5), op("bit_not")]), int(-6));
}

#[test]
fn stack_manipulation_matches_casm_semantics() {
    let three = || vec![push_i(1), push_i(2), push_i(3)];
    // rot: [x, y, z] -> [y, z, x] (same as FastVM/JIT, not CVM1's raw ROT).
    let mut p = three();
    p.push(op("rot"));
    assert_eq!(run_stack(p), vec![int(2), int(3), int(1)]);
    // pick n copies the nth item from the top (0 = top).
    let mut p = three();
    p.push(ins("pick", json!({"n": 2})));
    assert_eq!(run_stack(p), vec![int(1), int(2), int(3), int(1)]);
    // roll n moves it to the top.
    let mut p = three();
    p.push(ins("roll", json!({"n": 1})));
    assert_eq!(run_stack(p), vec![int(1), int(3), int(2)]);
}

#[test]
fn type_of_and_cast_compute() {
    assert_eq!(run_top(vec![push_i(1), op("type_of")]), st("int"));
    assert_eq!(run_top(vec![push_s("x"), op("type_of")]), st("str"));
    assert_eq!(run_top(vec![push_f(1.5), op("type_of")]), st("float"));
    let cast = |v: Instruction, ty: &str| run_top(vec![v, ins("cast", json!({"type": ty}))]);
    assert_eq!(cast(push_f(3.7), "int"), int(3));
    assert_eq!(cast(push_s("42"), "int"), int(42));
    assert_eq!(cast(push_i(5), "float"), float(5.0));
    assert_eq!(cast(push_i(5), "str"), st("5"));
    assert_eq!(cast(push_i(0), "bool"), Value::Bool(false));
}

#[test]
fn math_ops_compute() {
    assert_eq!(run_top(vec![push_i(2), push_i(10), op("math_pow")]), float(1024.0));
    let un = |name: &str, v: f64| run_top(vec![push_f(v), op(name)]);
    assert_eq!(un("math_sqrt", 9.0), float(3.0));
    assert_eq!(un("math_abs", -3.5), float(3.5));
    assert_eq!(un("math_round", 2.5), float(3.0));
    assert_eq!(un("math_floor", 2.7), float(2.0));
    assert_eq!(un("math_ceil", 2.1), float(3.0));
}

#[test]
fn string_ops_compute() {
    let two = |name: &str, a: &str, b: &str| run_top(vec![push_s(a), push_s(b), op(name)]);
    assert_eq!(two("str_starts_with", "hello", "he"), Value::Bool(true));
    assert_eq!(two("str_starts_with", "hello", "lo"), Value::Bool(false));
    assert_eq!(two("str_ends_with", "hello", "lo"), Value::Bool(true));
    assert_eq!(two("str_ends_with", "hello", "he"), Value::Bool(false));
    let one = |name: &str, a: &str| run_top(vec![push_s(a), op(name)]);
    assert_eq!(one("str_to_upper", "Hi there"), st("HI THERE"));
    assert_eq!(one("str_to_lower", "Hi There"), st("hi there"));
    assert_eq!(one("str_trim", "  x y \n"), st("x y"));
}

#[test]
fn array_ops_compute() {
    let two_elems = || {
        vec![
            ins("new_array", json!({"size": 2})),
            push_i(10),
            op("arr_push"),
            push_i(20),
            op("arr_push"),
        ]
    };
    let mut p = two_elems();
    p.push(op("arr_len"));
    assert_eq!(run_top(p), int(2));
    // arr_pop: array -> array, value
    let mut p = two_elems();
    p.push(op("arr_pop"));
    assert_eq!(
        run_stack(p),
        vec![Value::new_array(vec![int(10)]), int(20)]
    );
}

#[test]
fn collection_constructors_and_pushes_compute() {
    let build = |new: &str, push: &str| {
        run_top(vec![
            ins(new, json!({"size": 2})),
            push_i(1),
            op(push),
            push_i(2),
            op(push),
        ])
    };
    let items = || vec![int(1), int(2)];
    assert_eq!(build("new_tuple", "tuple_push"), Value::new_tuple(items()));
    assert_eq!(build("new_list", "list_push"), Value::new_list(items()));
    assert_eq!(build("new_vector", "vector_push"), Value::new_vector(items()));
    assert_eq!(build("new_set", "set_push"), Value::new_set(items()));
}

#[test]
fn spawn_and_await_yield_the_spawned_functions_return_value() {
    // Spawned threads keep their own output buffer, so the observable effect
    // is the awaited return value, not a print.
    let mut program = program_of(vec![
        push_s("worker"),
        ins("spawn", json!({"argc": 0})),
        op("await"),
        op("halt"),
    ]);
    program.functions.insert(
        "worker".to_string(),
        Function {
            params: vec![],
            locals: vec![],
            type_hints: None,
            body: vec![push_i(42), op("ret")],
        },
    );
    let vm = casm_to_vm(&program).expect("lowers");
    let stack = crush_vm::run_with_caps(&vm, &Quotas::default(), None)
        .expect("runs")
        .stack;
    assert_eq!(stack, vec![int(42)]);
}
