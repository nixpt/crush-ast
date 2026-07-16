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
    assert_eq!(out.lines().collect::<Vec<_>>(), vec!["8"], "unexpected output: {out}");
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
