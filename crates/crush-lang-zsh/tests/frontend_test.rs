use crush_lang_zsh::ZshFrontend;
use walker_core::Frontend;
use crush_cast::{Statement, Expression};

fn test_analyze(source: &str) -> walker_core::FeatureReport {
    let frontend = ZshFrontend;
    let ast = frontend.parse(source).unwrap();
    frontend.analyze(&ast).unwrap()
}

#[test]
fn test_zsh_detects_dangerous_commands() {
    let report = test_analyze("eval \"ls -la\"\necho hi\n");
    assert!(report.dangerous_imports.contains(&"eval".to_string()));
    assert!(!report.can_lower_safely());
}

#[test]
fn test_zsh_safe_code() {
    let source = "NAME=\"World\"\necho \"Hello\"\n";
    let frontend = ZshFrontend;
    let (report, program) = walker_core::frontend_pipeline(&frontend, source).unwrap();
    assert!(report.can_lower_safely());
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_zsh_detects_functions() {
    let report = test_analyze("my_func() {\n    echo hi\n}\n");
    assert!(report.uses_functions);
}

#[test]
fn test_zsh_detects_side_effects() {
    let report = test_analyze("MYVAR=hello\necho \"$MYVAR\"\n");
    assert!(report.has_top_level_side_effects);
}

#[test]
fn test_zsh_echo_uses_io_print() {
    let cast = crush_lang_zsh::zsh_to_cast("echo hello\n").unwrap();
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
fn test_zsh_variable_reference() {
    let cast = crush_lang_zsh::zsh_to_cast("NAME=\"World\"\necho \"Hello, $NAME!\"\n").unwrap();
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
fn test_zsh_unset_lowers_to_null() {
    let cast = crush_lang_zsh::zsh_to_cast("unset MYVAR\n").unwrap();
    let main = cast.functions.get("main").unwrap();
    assert_eq!(main.body.len(), 1);
    if let Some(Statement::VarDecl { name, value, .. }) = main.body.first() {
        assert_eq!(name, "MYVAR");
        assert!(matches!(value, Expression::NullLiteral { .. }));
    } else {
        panic!("expected VarDecl with NullLiteral");
    }
}

#[test]
fn test_zsh_source_lowers_to_capability() {
    let cast = crush_lang_zsh::zsh_to_cast("source ./setup.sh\n").unwrap();
    let main = cast.functions.get("main").unwrap();
    assert_eq!(main.body.len(), 1);
    if let Some(Statement::ExprStmt { expr, .. }) = main.body.first() {
        if let Expression::CapabilityCall { name, args, .. } = expr {
            assert_eq!(name, "bash.source");
            assert_eq!(args.len(), 1);
            if let Expression::StringLiteral { value, .. } = &args[0] {
                assert_eq!(value, "./setup.sh");
            } else {
                panic!("expected StringLiteral arg");
            }
        } else {
            panic!("expected CapabilityCall");
        }
    } else {
        panic!("expected ExprStmt");
    }
}

#[test]
fn test_zsh_return_lowers_to_return() {
    let cast = crush_lang_zsh::zsh_to_cast("return 42\n").unwrap();
    let main = cast.functions.get("main").unwrap();
    assert_eq!(main.body.len(), 1);
    if let Some(Statement::Return { value, .. }) = main.body.first() {
        assert!(value.is_some());
        if let Some(Expression::StringLiteral { value, .. }) = value {
            assert_eq!(value, "42");
        } else {
            panic!("expected StringLiteral value");
        }
    } else {
        panic!("expected Return");
    }
}

#[test]
fn test_zsh_if_else() {
    let cast = crush_lang_zsh::zsh_to_cast("if true; then echo yes; else echo no; fi\n").unwrap();
    let main = cast.functions.get("main").unwrap();
    assert_eq!(main.body.len(), 1);
    if let Some(Statement::If { condition, then_body, else_body, .. }) = main.body.first() {
        assert!(matches!(condition, Expression::BoolLiteral { value: true, .. }));
        assert_eq!(then_body.len(), 1);
        assert!(else_body.is_some());
    } else {
        panic!("expected If");
    }
}

#[test]
fn test_zsh_subshell() {
    let cast = crush_lang_zsh::zsh_to_cast("(echo hello; echo world)\n").unwrap();
    let main = cast.functions.get("main").unwrap();
    assert_eq!(main.body.len(), 2, "subshell body statements should be inlined");
}

#[test]
fn test_zsh_function_def() {
    let source = "greet() { echo \"Hello\"; }\ngreet\n";
    let cast = crush_lang_zsh::zsh_to_cast(source).unwrap();
    assert!(cast.functions.contains_key("greet"));
    let main = cast.functions.get("main").unwrap();
    assert_eq!(main.body.len(), 1);
}

#[test]
fn test_zsh_case_statement() {
    let cast = crush_lang_zsh::zsh_to_cast("case $x in a) echo 1;; b) echo 2;; esac\n").unwrap();
    let main = cast.functions.get("main").unwrap();
    assert_eq!(main.body.len(), 2, "case should produce one If per branch");
    for (i, stmt) in main.body.iter().enumerate() {
        match stmt {
            Statement::If { condition, then_body, .. } => {
                assert!(matches!(condition, Expression::BinaryOp { operator, .. } if operator == "=="));
                assert_eq!(then_body.len(), 1, "each case branch should have one stmt");
            }
            _ => panic!("expected If for branch {i}, got {stmt:?}"),
        }
    }
}

#[test]
fn test_zsh_for_loop() {
    let cast = crush_lang_zsh::zsh_to_cast("for x in a b c; do echo $x; done\n").unwrap();
    let main = cast.functions.get("main").unwrap();
    assert_eq!(main.body.len(), 1);
    if let Some(Statement::For { variable, iterable, body, .. }) = main.body.first() {
        assert_eq!(variable, "x");
        if let Expression::ArrayLiteral { elements, .. } = iterable.as_ref() {
            assert_eq!(elements.len(), 3);
        } else {
            panic!("expected ArrayLiteral iterable");
        }
        assert_eq!(body.len(), 1);
    } else {
        panic!("expected For");
    }
}

#[test]
fn test_zsh_while_loop() {
    let cast = crush_lang_zsh::zsh_to_cast("while true; do echo looping; done\n").unwrap();
    let main = cast.functions.get("main").unwrap();
    assert_eq!(main.body.len(), 1);
    if let Some(Statement::While { condition, body, .. }) = main.body.first() {
        assert!(matches!(condition.as_ref(), Expression::BoolLiteral { value: true, .. }));
        assert_eq!(body.len(), 1);
    } else {
        panic!("expected While");
    }
}
