fn main() {
    let out = std::path::PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").unwrap(),
    ).join("opcodes.json");

    let spec = serde_json::json!({
        "magic": "CVM1",
        "version": 2,
        "min_version": 1,
        "opcodes": {
            "NOP":      0x00,
            "PUSH":     0x01,
            "PUSH_STR": 0x02,
            "POP":      0x03,
            "DUP":      0x04,
            "SWAP":     0x05,
            "PUSH_F64": 0x06,
            "PUSH_NULL": 0x07,
            "ADD":      0x10,
            "SUB":      0x11,
            "MUL":      0x12,
            "DIV":      0x13,
            "MOD":      0x14,
            "EQ":       0x20,
            "LT":       0x21,
            "GT":       0x22,
            "NOT":      0x23,
            "LOAD":     0x30,
            "STORE":    0x31,
            "JMP":      0x40,
            "JZ":       0x41,
            "JNZ":      0x42,
            "PRINT":    0x50,
            "CAP_CALL": 0x51,
            "CALL":     0x52,
            "RET":      0x53,
            "NEW_ARRAY": 0x60,
            "ARR_GET":  0x61,
            "ARR_SET":  0x62,
            "ARR_LEN":  0x63,
            "EXEC_LANG": 0x70,
            "HALT":     0xFF
        },
        "operand_kinds": {
            "PUSH":     "i64",
            "PUSH_F64": "f64",
            "PUSH_STR": "str",
            "LOAD":     "slot",
            "STORE":    "slot",
            "JMP":      "addr",
            "JZ":       "addr",
            "JNZ":      "addr",
            "CAP_CALL": "cap",
            "CALL":     "func",
            "EXEC_LANG": "str",
            "NEW_ARRAY": "count"
        }
    });

    std::fs::write(&out, serde_json::to_string_pretty(&spec).unwrap())
        .expect("write opcodes.json");
    println!("cargo:rerun-if-changed=build.rs");
}
