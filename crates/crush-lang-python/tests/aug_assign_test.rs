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

/// Runs `source` through both front doors and returns their output.
fn run(source: &str) -> (String, String) {
    let cast = crush_lang_python::python_to_cast(source).expect("python to cast");
    let exec = |casm| {
        let vm = crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm to vm");
        crush_vm::run_with_caps(&vm, &crush_vm::Quotas::default(), None)
            .expect("vm run")
            .output
            .trim()
            .to_string()
    };
    let checked = crush_frontend::compile_cast(&cast).expect("compile_cast");
    let walker = crush_frontend::compiler::Compiler::new()
        .compile(cast)
        .expect("Compiler::compile");
    (exec(checked), exec(walker))
}

/// `a[i] op= v` (#425) used to be refused ("augmented assignment target must
/// be a name") while `a[i] = a[i] op v` ran. It writes through to the list,
/// so an alias sees the change, the same as in Python.
#[test]
fn subscript_augmented_assignment_writes_through() {
    let (checked, walker) = run(
        "a = [1, 2]\nb = a\na[0] += 5\na[1] *= 3\nd = {'k': 1}\nd['k'] -= 4\nprint(a, b, d['k'])\n",
    );
    assert_eq!(checked, "[6, 6] [6, 6] -3");
    assert_eq!(walker, checked);
}

/// The target's object and index are evaluated once, as in Python: a call in
/// either position runs one time, not once for the read and once for the write.
#[test]
fn subscript_augmented_assignment_evaluates_the_target_once() {
    let source = "\
def obj(log, xs):
    log[0] = log[0] + 1
    return xs
def idx(log):
    log[1] = log[1] + 1
    return 1
calls = [0, 0]
a = [10, 20]
obj(calls, a)[idx(calls)] += 5
print(a, calls)
";
    let (checked, walker) = run(source);
    assert_eq!(checked, "[10, 25] [1, 1]");
    assert_eq!(walker, checked);
}

#[test]
fn slice_augmented_assignment_is_refused() {
    let err = crush_lang_python::python_to_cast("a = [1, 2]\na[0:1] += [3]\n").unwrap_err();
    assert!(err.to_string().contains("slice"), "{err}");
}
