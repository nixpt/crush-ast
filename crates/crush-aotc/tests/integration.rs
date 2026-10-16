//! Runtime integration tests for crush-aotc.
//!
//! These tests generate C source, compile it with the system C compiler,
//! execute the resulting binary, and verify stdout.

use std::process::Command;

use crush_aotc::{AotcCompiler, AotcOpts};

fn compile_and_run(source: &str) -> anyhow::Result<String> {
    let program = crush_frontend::compile_crush_source(source)?;
    let c_source = AotcCompiler::new(AotcOpts::default()).compile(&program)?;
    eprintln!("=== GENERATED C ===\n{}", c_source);

    let dir = tempfile::tempdir()?;
    let c_path = dir.path().join("test.c");
    let exe_path = dir.path().join("test");
    std::fs::write(&c_path, c_source)?;

    let status = Command::new("cc")
        .args([&c_path.to_string_lossy(), "-o", &exe_path.to_string_lossy(), "-lm"])
        .status()?;
    if !status.success() {
        anyhow::bail!("cc failed with exit code {:?}", status.code());
    }

    let output = Command::new(&exe_path).output()?;
    if !output.status.success() {
        anyhow::bail!("executable failed with exit code {:?}", output.status.code());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// `source` with the first `store` to `local` in `func` sent to a dead local,
/// compiled and run: `(exit ok, stdout, stderr)`. (The frontend refuses a
/// read of an unassigned local; a walker can emit one — CRUSH-zorro-55.)
fn run_unassigned(source: &str, func: &str, local: &str) -> (bool, String, String) {
    let mut program = crush_frontend::compile_crush_source(source).expect("frontend");
    let f = program.functions.get_mut(func).expect("function");
    let store = f
        .body
        .iter_mut()
        .find(|i| i.op == "store" && i.args.get("name").and_then(|n| n.as_str()) == Some(local))
        .expect("a store to the local");
    store.args["name"] = serde_json::Value::from("__dead");
    let c_source = AotcCompiler::new(AotcOpts::default()).compile(&program).expect("aotc");
    let dir = tempfile::tempdir().unwrap();
    let (c_path, exe_path) = (dir.path().join("t.c"), dir.path().join("t"));
    std::fs::write(&c_path, &c_source).unwrap();
    let status = Command::new("cc")
        .args([&c_path.to_string_lossy(), "-o", &exe_path.to_string_lossy(), "-lm"])
        .status()
        .unwrap();
    assert!(status.success(), "cc failed:\n{c_source}");
    let o = Command::new(&exe_path).output().unwrap();
    (o.status.success(), String::from_utf8_lossy(&o.stdout).into(), String::from_utf8_lossy(&o.stderr).into())
}

/// CRUSH-zorro-55: a read of an unassigned local fails, as on the VMs,
/// instead of reading the `= 0` it used to be declared with.
#[test]
fn unassigned_local_read_fails() {
    let (ok, _, err) = run_unassigned("fn n(v) { return v; }\nfn main() { let x = n(5)\n print(x) }\n", "main", "x");
    assert!(!ok && err.contains("load from uninitialised slot `x`"), "{err}");

    let src = |arg: &str| format!("fn f(c) {{ let x = 0\n if c {{ x = 7 }}\n return x }}\nfn main() {{ print(f({arg})) }}\n");
    let (ok, _, err) = run_unassigned(&src("false"), "f", "x");
    assert!(!ok && err.contains("load from uninitialised slot `x`"), "{err}");
    // ... and the path that assigns it still reads the value.
    let (ok, out, err) = run_unassigned(&src("true"), "f", "x");
    assert!(ok && out.trim() == "7", "{out} {err}");
}

#[test]
fn string_equality_true() {
    let source = r#"
        fn main() {
            let a = "hello"
            let b = "hello"
            let c = (a == b)
            print(c)
        }
    "#;
    let out = compile_and_run(source).expect("compile and run");
    assert!(out.contains("true"), "expected 'true', got: {out}");
}

#[test]
fn string_equality_false() {
    let source = r#"
        fn main() {
            let a = "hello"
            let b = "world"
            let c = (a == b)
            print(c)
        }
    "#;
    let out = compile_and_run(source).expect("compile and run");
    assert!(out.contains("false"), "expected 'false', got: {out}");
}

#[test]
fn cross_type_equality_false() {
    // The frontend type-checks literal comparisons, so route through `any`
    // parameters to exercise the runtime cross-type path.
    let source = r#"
        fn eq_any(a: any, b: any) { return a == b }
        fn main() {
            print(eq_any(1, "1"))
        }
    "#;
    let out = compile_and_run(source).expect("compile and run");
    assert!(out.contains("false"), "expected 'false', got: {out}");
}

#[test]
fn bool_equality_and_null_equality() {
    let source = r#"
        fn main() {
            print(true == true)
            print(false == false)
            print(true == false)
            print(null == null)
        }
    "#;
    let out = compile_and_run(source).expect("compile and run");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines, vec!["true", "true", "false", "true"], "unexpected output: {out}");
}

#[test]
fn scalar_param_function_add() {
    let source = r#"
        fn add(a, b) { return a + b }
        fn main() {
            print(add(2, 3))
        }
    "#;
    let out = compile_and_run(source).expect("compile and run");
    assert_eq!(out.lines().collect::<Vec<_>>(), vec!["5"], "unexpected output: {out}");
}

#[test]
fn argument_order_is_preserved() {
    let source = r#"
        fn sub(a, b) { return a - b }
        fn main() {
            print(sub(5, 3))
            print(sub(3, 5))
        }
    "#;
    let out = compile_and_run(source).expect("compile and run");
    assert_eq!(out.lines().collect::<Vec<_>>(), vec!["2", "-2"], "unexpected output: {out}");
}

#[test]
fn scalar_float_return() {
    let source = r#"
        fn main() {
            print(5.0 / 2)
        }
    "#;
    let out = compile_and_run(source).expect("compile and run");
    assert_eq!(out.lines().collect::<Vec<_>>(), vec!["2.5"], "unexpected output: {out}");
}

#[test]
fn mixed_type_arithmetic() {
    let source = r#"
        fn main() {
            print(2 + 5.0)
            print(5.0 + 2)
            print(5.0 / 2)
            print(7 % 3.0)
        }
    "#;
    let out = compile_and_run(source).expect("compile and run");
    assert_eq!(out.lines().collect::<Vec<_>>(), vec!["7.0", "7.0", "2.5", "1.0"], "unexpected output: {out}");
}

#[test]
fn mixed_type_comparison() {
    let source = r#"
        fn main() {
            print(5.0 > 2)
            print(2 < 5.0)
        }
    "#;
    let out = compile_and_run(source).expect("compile and run");
    assert_eq!(out.lines().collect::<Vec<_>>(), vec!["true", "true"], "unexpected output: {out}");
}

#[test]
fn capability_argument_order() {
    let source = r#"
        fn main() {
            print(math.pow(2, 3))
        }
    "#;
    let out = compile_and_run(source).expect("compile and run");
    // math.pow returns a float (matching FastVM), and io.print formats whole
    // floats with a trailing ".0" (matching runtime_value_to_text in io_print.rs).
    assert_eq!(out.lines().collect::<Vec<_>>(), vec!["8.0"], "unexpected output: {out}");
}

#[test]
fn ordered_comparison_on_string_is_rejected() {
    let source = r#"
        fn lt_any(a: any, b: any) { return a < b }
        fn main() {
            print(lt_any("a", "b"))
        }
    "#;
    let result = compile_and_run(source);
    assert!(result.is_err(), "ordered comparison on strings should be rejected");
}

#[test]
fn truthiness_is_canonical() {
    // CRUSH-134: null, false, 0, 0.0 and "" are falsy; everything else truthy.
    let source = r#"
        fn t(v: any) { if v { return 1 } return 0 }
        fn main() {
            print(t(null)) print(t(false)) print(t(true))
            print(t(0)) print(t(7))
            print(t(0.0)) print(t(1.5))
            print(t("")) print(t("x"))
        }
    "#;
    let out = compile_and_run(source).expect("compile and run");
    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        ["0", "0", "1", "0", "1", "0", "1", "0", "1"],
        "{out}"
    );
}

/// COSM K1.3 (CRUSH-zorro-42): a kind table whose Kinds all behave as plain
/// maps (Crush's defaults) compiles and runs like the scheduler VM; a Kind a
/// map can't represent is refused before any C is generated.
const COSM_POINT: &str = r#"
struct Point { x: int, y: int }
impl Point {
    fn new(x, y) { return Point { x: x, y: y }; }
    fn norm2(self) { return self.x * self.x + self.y * self.y; }
}
trait Shape { fn area(self); fn sides(self) { return 4; } }
impl Shape for Point { fn area(self) { return 100; } }
fn main() {
    let p = Point::new(3, 4)
    print(Point::norm2(p) + Point::Shape::area(p) + Shape::sides(p))
}
"#;

/// The COSM rule lets a map-compatible kind table through on crush-aotc, as on
/// every engine. crush-aotc has no objects or maps at all, though: the struct
/// instances are refused as object ops — loudly, where they used to compile to
/// C that read every field as the last value set (this program printed 136,
/// not the VM's 129).
#[test]
fn a_map_compatible_kind_table_passes_the_cosm_rule_but_objects_are_refused() {
    let program = crush_frontend::compile_crush_source(COSM_POINT).expect("frontend");
    assert!(program.kind_table().is_some());
    assert_eq!(program.refuse_cosm("crush-aotc"), Ok(()));
    let err = AotcCompiler::new(AotcOpts::default()).compile(&program).expect_err("objects refused");
    let err = err.to_string();
    assert!(!err.contains("COSM"), "{err}");
    assert!(err.contains("(objects and maps, including struct instances) isn't supported"), "{err}");
    // The same refusal for a plain map, without any kind table.
    let plain = crush_frontend::compile_crush_source("fn main() { let p = {x: 3, y: 4}\n print(p.x) }\n").unwrap();
    assert!(AotcCompiler::new(AotcOpts::default()).compile(&plain).is_err());
}

#[test]
fn a_kind_maps_cant_represent_is_refused() {
    let mut program = crush_frontend::compile_crush_source(COSM_POINT).expect("frontend");
    let mut v = serde_json::to_value(program.cosm.as_ref().unwrap()).unwrap();
    v["kinds"][0]["equality"] = serde_json::json!("identity");
    program.cosm = Some(serde_json::from_value(v).unwrap());
    let err = AotcCompiler::new(AotcOpts::default()).compile(&program).expect_err("refused");
    let err = err.to_string();
    assert!(err.starts_with("COSM objects aren't supported on crush-aotc yet (CRUSH-zorro-43)"), "{err}");
    assert!(err.contains("`Point` declares equality: identity"), "{err}");
}

/// issue #161: no green-task scheduler here — `spawn` is refused by name, not
/// compiled to `unhandled op` C that silently skips the task. `yield`/`await`
/// without `spawn` are no-ops (a lone `yield` has nothing to yield to, and an
/// `await` has no handle), so they compile, not refuse.
#[test]
fn green_tasks_are_refused_by_name() {
    let program = crush_frontend::compile_crush_source("fn worker() {}\nfn main() { spawn worker() }\n")
        .expect("frontend");
    let err = AotcCompiler::new(AotcOpts::default()).compile(&program).expect_err("refused");
    let err = err.to_string();
    assert!(err.contains("spawn"), "{err}");
    assert!(!err.contains("make_closure"), "{err}");
}

#[test]
fn lone_yield_and_await_compile_not_refuse() {
    for source in ["fn main() { yield }\n", "fn main() { let h = 1\n await h }\n"] {
        let program = crush_frontend::compile_crush_source(source).expect("frontend");
        AotcCompiler::new(AotcOpts::default()).compile(&program).expect("compiles: a lone yield/await is a no-op");
    }
}
