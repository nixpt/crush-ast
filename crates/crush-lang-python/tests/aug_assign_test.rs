//! `x op= y` lowers like `x = x op y` for every operator the plain form
//! accepts. `**=`, `<<=`, `>>=`, `|=`, `^=`, `&=` and `@=` used to be refused
//! with "unsupported augmented operator" although `x op y` lowered.

fn shape(source: &str) -> serde_json::Value {
    fn strip(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::Object(m) => {
                m.remove("meta");
                m.values_mut().for_each(strip);
            }
            serde_json::Value::Array(a) => a.iter_mut().for_each(strip),
            _ => {}
        }
    }
    let cast = crush_lang_python::python_to_cast(source).unwrap();
    let mut v = serde_json::to_value(&cast).unwrap();
    strip(&mut v);
    v
}

#[test]
fn augmented_assignment_lowers_like_its_expanded_form() {
    for op in [
        "+", "-", "*", "/", "//", "%", "**", "<<", ">>", "|", "^", "&", "@",
    ] {
        let augmented = shape(&format!("x = 9\nx {op}= 2\n"));
        let expanded = shape(&format!("x = 9\nx = x {op} 2\n"));
        assert_eq!(augmented, expanded, "`{op}=`");
    }
}
