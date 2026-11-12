//! Every compound assignment lowers to `x = x op y`. `&=`, `|=`, `^=`, `<<=`
//! and `>>=` used to fail with "unsupported binary operator" although `&`,
//! `|`, `^`, `<<` and `>>` lowered.

use crush_lang_rust::rust_to_cast;

fn binary_operators(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Object(m) => {
            if let Some(serde_json::Value::String(op)) = m.get("operator") {
                out.push(op.clone());
            }
            m.values().for_each(|x| binary_operators(x, out));
        }
        serde_json::Value::Array(a) => a.iter().for_each(|x| binary_operators(x, out)),
        _ => {}
    }
}

#[test]
fn every_compound_assignment_lowers_to_its_base_operator() {
    for op in ["+", "-", "*", "/", "%", "&", "|", "^", "<<", ">>"] {
        let source =
            format!("fn main() {{ let mut x: i64 = 9; x {op}= 2; println!(\"{{}}\", x); }}");
        let cast = rust_to_cast(&source).unwrap_or_else(|e| panic!("`{op}=`: {e:#}"));
        let mut ops = Vec::new();
        binary_operators(&serde_json::to_value(&cast).unwrap(), &mut ops);
        assert!(
            ops.iter().any(|o| o == op),
            "`{op}=` lowered without `{op}`: {ops:?}"
        );
    }
}
