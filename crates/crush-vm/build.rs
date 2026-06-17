fn main() {
    use std::collections::BTreeMap;

    let out = std::path::PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").unwrap(),
    ).join("opcodes.json");

    let mut opcodes: BTreeMap<&str, u64> = BTreeMap::new();
    opcodes.insert("NOP", 0);
    opcodes.insert("PUSH", 1);
    opcodes.insert("PUSH_STR", 2);
    opcodes.insert("POP", 3);
    opcodes.insert("DUP", 4);
    opcodes.insert("SWAP", 5);
    opcodes.insert("PUSH_F64", 6);
    opcodes.insert("PUSH_NULL", 7);
    opcodes.insert("PUSH_BOOL", 8);
    opcodes.insert("ADD", 16);
    opcodes.insert("SUB", 17);
    opcodes.insert("MUL", 18);
    opcodes.insert("DIV", 19);
    opcodes.insert("MOD", 20);
    opcodes.insert("EQ", 32);
    opcodes.insert("LT", 33);
    opcodes.insert("GT", 34);
    opcodes.insert("NOT", 35);
    opcodes.insert("LOAD", 48);
    opcodes.insert("STORE", 49);
    opcodes.insert("JMP", 64);
    opcodes.insert("JZ", 65);
    opcodes.insert("JNZ", 66);
    opcodes.insert("PRINT", 80);
    opcodes.insert("CAP_CALL", 81);
    opcodes.insert("CALL", 82);
    opcodes.insert("RET", 83);
    opcodes.insert("NEW_ARRAY", 96);
    opcodes.insert("ARR_GET", 97);
    opcodes.insert("ARR_SET", 98);
    opcodes.insert("ARR_LEN", 99);
    opcodes.insert("ARR_PUSH", 100);
    opcodes.insert("ARR_POP", 101);
    opcodes.insert("EXEC_LANG", 112);
    opcodes.insert("NEW_OBJ", 113);
    opcodes.insert("SET_FIELD", 114);
    opcodes.insert("GET_FIELD", 115);
    opcodes.insert("HALT", 255);

    let mut okinds: BTreeMap<&str, &str> = BTreeMap::new();
    okinds.insert("PUSH", "i64");
    okinds.insert("PUSH_F64", "f64");
    okinds.insert("PUSH_STR", "str");
    okinds.insert("PUSH_BOOL", "i64");
    okinds.insert("LOAD", "slot");
    okinds.insert("STORE", "slot");
    okinds.insert("JMP", "addr");
    okinds.insert("JZ", "addr");
    okinds.insert("JNZ", "addr");
    okinds.insert("CAP_CALL", "cap");
    okinds.insert("CALL", "func");
    okinds.insert("EXEC_LANG", "str");
    okinds.insert("SET_FIELD", "str");
    okinds.insert("GET_FIELD", "str");
    okinds.insert("NEW_ARRAY", "count");

    let spec = serde_json::json!({
        "magic": "CVM1",
        "version": 2,
        "min_version": 1,
        "opcodes": opcodes,
        "operand_kinds": okinds,
    });

    std::fs::write(&out, serde_json::to_string_pretty(&spec).unwrap())
        .expect("write opcodes.json");
    println!("cargo:rerun-if-changed=build.rs");
}
