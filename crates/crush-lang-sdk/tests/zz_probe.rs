#[test]
fn probe() {
    for e in ["str.starts_with(\"ab\",\"a\")", "str.contains(\"ab\",\"a\")"] {
        let casm = crush_lang_sdk::compile::compile_crush_to_casm(&format!("fn main() {{ return {e}; }}")).unwrap();
        for f in casm.functions.values() { for i in &f.body { println!("{e} :: {} {}", i.op, i.args); } }
        println!("fast: {:?}", crush_vm::run_fastvm(&casm));
    }
}
