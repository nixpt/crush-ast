use crush_lang_bash::BashFrontend;
use walker_core::Frontend;
use crush_cast::{Statement, Expression};

fn test_analyze(source: &str) -> walker_core::FeatureReport {
    let frontend = BashFrontend;
    let ast = frontend.parse(source).unwrap();
    frontend.analyze(&ast).unwrap()
}

#[test]
fn test_bash_detects_dangerous_commands() {
    let report = test_analyze("eval \"ls -la\"\necho hi\n");
    assert!(report.dangerous_imports.contains(&"eval".to_string()));
    assert!(!report.can_lower_safely());
}

#[test]
fn test_bash_safe_code() {
    let source = "NAME=\"World\"\necho \"Hello\"\n";
    let frontend = BashFrontend;
    let (report, program) = walker_core::frontend_pipeline(&frontend, source).unwrap();
    assert!(report.can_lower_safely());
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_bash_detects_functions() {
    let report = test_analyze("my_func() {\n    echo hi\n}\n");
    assert!(report.uses_functions);
}

#[test]
fn test_bash_detects_side_effects() {
    let report = test_analyze("MYVAR=hello\necho \"$MYVAR\"\n");
    assert!(report.has_top_level_side_effects);
}

#[test]
fn test_bash_echo_uses_io_print() {
    let cast = crush_lang_bash::bash_to_cast("echo hello\n").unwrap();
    let main = cast.functions.get("main").unwrap();
    assert_eq!(main.body.len(), 1);
    if let Some(Statement::ExprStmt { expr, .. }) = main.body.first() {
        if let Expression::CapabilityCall { name, .. } = expr {
            assert_eq!(name, "io.print");
        } else {
            panic!("expected CapabilityCall, got {:?}", expr);
        }
    } else {
        panic!("expected ExprStmt");
    }
}

#[test]
fn test_bash_variable_reference() {
    let cast = crush_lang_bash::bash_to_cast("NAME=\"World\"\necho \"Hello, $NAME!\"\n").unwrap();
    let main = cast.functions.get("main").unwrap();
    assert_eq!(main.body.len(), 2);

    if let Some(Statement::VarDecl { name, value, .. }) = main.body.first() {
        assert_eq!(name, "NAME");
        if let Expression::StringLiteral { value, .. } = value {
            assert_eq!(value, "World");
        } else {
            panic!("expected StringLiteral");
        }
    } else {
        panic!("expected VarDecl");
    }

    if let Some(Statement::ExprStmt { expr, .. }) = main.body.get(1) {
        if let Expression::CapabilityCall { args, .. } = expr {
            assert_eq!(args.len(), 1);
            let arg = &args[0];
            let has_var_ref = match arg {
                Expression::BinaryOp { .. } => true,
                Expression::Var { name, .. } if name == "NAME" => true,
                _ => false,
            };
            assert!(has_var_ref, "expected Var or BinaryOp with Var ref");
        } else {
            panic!("expected CapabilityCall");
        }
    }
}

#[test]
fn test_bash_function_def() {
    let source = "greet() {\n    echo \"Hello\"\n}\ngreet\n";
    let cast = crush_lang_bash::bash_to_cast(source).unwrap();
    assert!(cast.functions.contains_key("greet"));
    let main = cast.functions.get("main").unwrap();
    assert_eq!(main.body.len(), 1);
}
