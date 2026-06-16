use serde_json::json;
fn main() {
    let args = json!({
        "code": "\n        console.log(`Hello`);\n        let x = 2 + 2;\n",
        "lang": "javascript",
        "var_count": 0
    });
    let json = serde_json::to_string(&args).unwrap();
    println!("JSON: {:?}", json);
    println!("len: {}", json.len());
    // Test if assembler can parse it
    let escaped = json.replace('\\', "\\\\").replace('"', "\\\"");
    let assembly = format!("EXEC_LANG \"{escaped}\"");
    println!("Assembly: {}", &assembly[..std::cmp::min(100, assembly.len())]);
    println!("Assembly len: {}", assembly.len());
}
