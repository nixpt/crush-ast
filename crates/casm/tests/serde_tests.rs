use casm::{Instruction, Program};
use serde_json::json;

#[test]
fn test_program_deserialization() {
    let json_data = r#"{
        "version": "1.0",
        "functions": {
            "main": {
                "params": [],
                "locals": [],
                "body": [
                    {"op": "push_int", "value": 42},
                    {"op": "cap_call", "name": "io.print", "argc": 1}
                ]
            }
        }
    }"#;

    let program: Program = serde_json::from_str(json_data).expect("Failed to deserialize program");

    assert_eq!(program.version, "1.0");
    assert!(program.functions.contains_key("main"));

    let main_fn = program.functions.get("main").unwrap();
    assert_eq!(main_fn.body.len(), 2);

    let inst1 = &main_fn.body[0];
    assert_eq!(inst1.op, "push_int");
    assert_eq!(inst1.args["value"], 42);

    let inst2 = &main_fn.body[1];
    assert_eq!(inst2.op, "cap_call");
    assert_eq!(inst2.args["name"], "io.print");
    assert_eq!(inst2.args["argc"], 1);
}

#[test]
fn test_instruction_serialization() {
    let inst = Instruction {
        op: "push_str".to_string(),
        lang: None,
        meta: None,
        args: json!({ "value": "hello" }),
    };

    let serialized = serde_json::to_string(&inst).expect("Failed to serialize");
    // We can't guarantee field order in JSON, so parse it back to value
    let val: serde_json::Value = serde_json::from_str(&serialized).unwrap();

    assert_eq!(val["op"], "push_str");
    assert_eq!(val["value"], "hello");
}

#[test]
fn test_metadata_preservation() {
    let json_data = r#"{
        "op": "add",
        "meta": { "line": 10, "file": "main.crush" }
    }"#;

    let inst: Instruction = serde_json::from_str(json_data).expect("Failed to deserialize");

    assert!(inst.meta.is_some());
    let meta = inst.meta.unwrap();
    assert_eq!(meta["line"], 10);
    assert_eq!(meta["file"], "main.crush");
}

#[test]
fn test_frontend_opcode_surface_converts_to_typed_opcodes() {
    use casm::OpCode;

    let cases = [
        (
            Instruction {
                op: "len".into(),
                lang: None,
                meta: None,
                args: json!({}),
            },
            OpCode::Len,
        ),
        (
            Instruction {
                op: "index".into(),
                lang: None,
                meta: None,
                args: json!({}),
            },
            OpCode::Index,
        ),
        (
            Instruction {
                op: "enter_try".into(),
                lang: None,
                meta: None,
                args: json!({"target": 4}),
            },
            OpCode::EnterTry,
        ),
        (
            Instruction {
                op: "throw".into(),
                lang: None,
                meta: None,
                args: json!({}),
            },
            OpCode::Throw,
        ),
        (
            Instruction {
                op: "ai_goal_decl".into(),
                lang: None,
                meta: None,
                args: json!({"name": "demo"}),
            },
            OpCode::AiGoalDeclaration(json!({"name": "demo"})),
        ),
    ];

    for (instruction, expected) in cases {
        assert_eq!(instruction.to_opcode().unwrap(), expected);
    }
}

#[test]
fn test_frontend_opcode_json_names_are_stable() {
    use casm::OpCode;

    assert_eq!(
        serde_json::to_string(&OpCode::AiGoalDeclaration(json!({}))).unwrap(),
        r#"{"ai_goal_decl":{}}"#
    );
    assert_eq!(
        serde_json::to_string(&OpCode::AiToolchain(json!({}))).unwrap(),
        r#"{"ai_tool_chain":{}}"#
    );
    assert_eq!(
        serde_json::to_string(&OpCode::AiKnowledgeSharing(json!({}))).unwrap(),
        r#"{"ai_knowledge_share":{}}"#
    );
}

#[test]
fn test_typed_literal_instruction_materializes_legacy_view() {
    use casm::OpCode;

    let meta = Some(json!({"line": 7}));
    let cases = [
        (OpCode::PushInt(42), "push_int", json!({"value": 42})),
        (OpCode::PushFloat(2.5), "push_float", json!({"value": 2.5})),
        (
            OpCode::PushStr("hello".into()),
            "push_str",
            json!({"value": "hello"}),
        ),
        (OpCode::PushBool(true), "push_bool", json!({"value": true})),
        (OpCode::PushNull, "push_null", json!({})),
    ];

    for (opcode, expected_op, expected_args) in cases {
        let instruction =
            Instruction::from_opcode(opcode, Some("crush".into()), meta.clone()).unwrap();
        assert_eq!(instruction.op, expected_op);
        assert_eq!(instruction.args, expected_args);
        assert_eq!(instruction.lang.as_deref(), Some("crush"));
        assert_eq!(instruction.meta, meta);
    }

    let load = Instruction::from_opcode(OpCode::Load("item".into()), None, None).unwrap();
    assert_eq!(load.op, "load");
    assert_eq!(load.args, json!({"name": "item"}));

    let store = Instruction::from_opcode(OpCode::Store("item".into()), None, None).unwrap();
    assert_eq!(store.op, "store");
    assert_eq!(store.args, json!({"name": "item"}));

    for (opcode, expected_op) in [
        (OpCode::Add, "add"),
        (OpCode::Sub, "sub"),
        (OpCode::Mul, "mul"),
        (OpCode::Div, "div"),
        (OpCode::Mod, "mod"),
        (OpCode::Neg, "neg"),
        (OpCode::Eq, "eq"),
        (OpCode::Ne, "ne"),
        (OpCode::Lt, "lt"),
        (OpCode::Gt, "gt"),
        (OpCode::Le, "le"),
        (OpCode::Ge, "ge"),
    ] {
        let instruction = Instruction::from_opcode(opcode, None, None).unwrap();
        assert_eq!(instruction.op, expected_op);
        assert_eq!(instruction.args, json!({}));
    }

    assert!(
        Instruction::from_opcode(
            OpCode::Await {
                handle: "task".into()
            },
            None,
            None
        )
        .is_err()
    );
}
