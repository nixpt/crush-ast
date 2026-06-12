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
