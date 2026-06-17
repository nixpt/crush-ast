use std::collections::HashMap;
use std::process::Command;

/// Build a representative CAST Program in Python via dataclasses, serialize it
/// with `dataclasses.asdict` → `json.dumps`, and verify Rust `serde_json` can
/// parse it back into an AST that matches the equivalent Rust construction.
#[test]
fn python_dataclass_round_trip() {
    let python_script = format!(
        r#"
import dataclasses, json, sys
sys.path.insert(0, "{}/python")
from cast_types import (
    Program, Function, VarDecl, Return, ExprStmt, If, BinaryOp, IntLiteral, Var,
    StringLiteral, Call, LangBlock, CastType,
)

program = Program(
    cast_version="1.0",
    entry="main",
    lang="crush",
    functions={{
        "main": Function(
            params=[],
            body=[
                VarDecl(
                    name="x",
                    value=BinaryOp(
                        operator="+",
                        left=IntLiteral(value=1),
                        right=IntLiteral(value=2),
                    ),
                ),
                If(
                    condition=BinaryOp(
                        operator=">",
                        left=Var(name="x"),
                        right=IntLiteral(value=0),
                    ),
                    then_body=[
                        Return(value=Var(name="x")),
                    ],
                ),
                ExprStmt(
                    expr=Call(
                        function="io.print",
                        args=[StringLiteral(value="hello")],
                    ),
                ),
                LangBlock(
                    lang="python",
                    code="print('hello')",
                ),
                Return(value=IntLiteral(value=0)),
            ],
        ),
    }},
)

json_str = json.dumps(dataclasses.asdict(program), indent=2)
print(json_str)
"#,
        env!("CARGO_MANIFEST_DIR")
    );

    let output = Command::new("python3")
        .arg("-c")
        .arg(&python_script)
        .output()
        .expect("python3 should be available");

    if !output.stderr.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Python stderr:\n{}", stderr);
    }
    assert!(
        output.status.success(),
        "Python round-trip script failed with exit code {:?}",
        output.status.code()
    );

    let python_json = String::from_utf8(output.stdout).expect("valid UTF-8 from Python");

    // Parse the Python JSON into a Rust Program.
    let from_python: crush_cast::Program =
        serde_json::from_str(&python_json).expect("Python JSON should deserialize into Program");

    // Build the same AST directly in Rust.
    let from_rust = crush_cast::Program {
        cast_version: "1.0".to_string(),
        entry: "main".to_string(),
        lang: Some("crush".to_string()),
        functions: {
            let mut m = HashMap::new();
            m.insert(
                "main".to_string(),
                crush_cast::Function {
                    params: vec![],
                    body: vec![
                        crush_cast::Statement::VarDecl {
                            name: "x".to_string(),
                            value: crush_cast::Expression::BinaryOp {
                                operator: "+".to_string(),
                                left: Box::new(crush_cast::Expression::IntLiteral {
                                    value: 1,
                                    meta: HashMap::new(),
                                }),
                                right: Box::new(crush_cast::Expression::IntLiteral {
                                    value: 2,
                                    meta: HashMap::new(),
                                }),
                                meta: HashMap::new(),
                            },
                            type_hint: crush_cast::CastType::Any,
                            meta: HashMap::new(),
                        },
                        crush_cast::Statement::If {
                            condition: crush_cast::Expression::BinaryOp {
                                operator: ">".to_string(),
                                left: Box::new(crush_cast::Expression::Var {
                                    name: "x".to_string(),
                                    meta: HashMap::new(),
                                }),
                                right: Box::new(crush_cast::Expression::IntLiteral {
                                    value: 0,
                                    meta: HashMap::new(),
                                }),
                                meta: HashMap::new(),
                            },
                            then_body: vec![crush_cast::Statement::Return {
                                value: Some(crush_cast::Expression::Var {
                                    name: "x".to_string(),
                                    meta: HashMap::new(),
                                }),
                                meta: HashMap::new(),
                            }],
                            else_body: None,
                            meta: HashMap::new(),
                        },
                        crush_cast::Statement::ExprStmt {
                            expr: crush_cast::Expression::Call {
                                function: "io.print".to_string(),
                                args: vec![crush_cast::Expression::StringLiteral {
                                    value: "hello".to_string(),
                                    meta: HashMap::new(),
                                }],
                                meta: HashMap::new(),
                            },
                            meta: HashMap::new(),
                        },
                        crush_cast::Statement::LangBlock {
                            lang: "python".to_string(),
                            code: "print('hello')".to_string(),
                            variables: vec![],
                            imports: vec![],
                            meta: HashMap::new(),
                        },
                        crush_cast::Statement::Return {
                            value: Some(crush_cast::Expression::IntLiteral {
                                value: 0,
                                meta: HashMap::new(),
                            }),
                            meta: HashMap::new(),
                        },
                    ],
                    meta: HashMap::new(),
                    annotations: None,
                },
            );
            m
        },
        ai_meta: None,
        manifest: None,
        exhaustive_sites: vec![],
    };

    // Compare by re-serializing both to canonical JSON.
    let json_from_python = serde_json::to_string_pretty(&from_python).unwrap();
    let json_from_rust = serde_json::to_string_pretty(&from_rust).unwrap();

    assert_eq!(
        json_from_python, json_from_rust,
        "Python-generated CAST JSON should round-trip to the same Rust AST as a native Rust construction"
    );
}

#[test]
fn python_import_variant_round_trip() {
    // Specifically verify that the `import_` field in Python maps correctly
    // to the `import` field in Rust via serde(alias).
    let python_script = format!(
        r#"
import dataclasses, json, sys
sys.path.insert(0, "{}/python")
from cast_types import Program, Function, Import, CrushModule

program = Program(
    cast_version="1.0",
    entry="main",
    functions={{
        "main": Function(
            body=[
                Import(
                    import_=CrushModule(module_path="math", alias="m"),
                ),
            ],
        ),
    }},
)
print(json.dumps(dataclasses.asdict(program)))
"#,
        env!("CARGO_MANIFEST_DIR")
    );

    let output = Command::new("python3")
        .arg("-c")
        .arg(&python_script)
        .output()
        .expect("python3 should be available");

    assert!(output.status.success());

    let python_json = String::from_utf8(output.stdout).unwrap();
    let program: crush_cast::Program = serde_json::from_str(&python_json).unwrap();

    // Verify the Import statement deserialized correctly.
    let main = program.functions.get("main").unwrap();
    assert_eq!(main.body.len(), 1);
    match &main.body[0] {
        crush_cast::Statement::Import { import, meta } => {
            match import {
                crush_cast::ImportStatement::CrushModule {
                    module_path, alias, ..
                } => {
                    assert_eq!(module_path, "math");
                    assert_eq!(alias.as_deref(), Some("m"));
                }
                other => panic!("Expected CrushModule, got {:?}", other),
            }
            assert!(meta.is_empty());
        }
        other => panic!("Expected Import statement, got {:?}", other),
    }
}

#[test]
fn python_bindings_are_up_to_date() {
    // Re-run the generator and assert no diff against the committed file.
    use std::path::PathBuf;
    use std::process::Stdio;

    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let committed = crate_root.join("python").join("cast_types.py");
    assert!(committed.exists(), "committed cast_types.py should exist");

    let committed_bytes = std::fs::read(&committed).unwrap();

    let temp_dir = std::env::temp_dir().join(format!("cast_types_check_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let temp_out = temp_dir.join("cast_types.py");

    let status = Command::new("cargo")
        .args(["run", "-p", "crush-cast", "--bin", "export-py"])
        .env("CRUSH_CAST_PYTHON_OUT", &temp_out)
        .current_dir(&crate_root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("cargo should be available");

    assert!(status.success(), "export-py should succeed");

    // Note: export-py writes to the fixed path inside the crate, not to an env var.
    // We compare the freshly-written file against the committed one.
    let fresh_bytes = std::fs::read(&committed).unwrap();

    assert_eq!(
        committed_bytes, fresh_bytes,
        "python/cast_types.py is out of date. Run `cargo run -p crush-cast --bin export-py` and commit the result."
    );
}
