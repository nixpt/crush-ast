use crush_cast::{Expression, Statement};
use crush_lang_zsh::ZshFrontend;
use crush_walker_core::Frontend;

fn test_analyze(source: &str) -> crush_walker_core::FeatureReport {
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
    let (report, program) = crush_walker_core::frontend_pipeline(&frontend, source).unwrap();
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

/// Every statement in `body`, nested ones included.
fn walk<'a>(body: &'a [Statement], out: &mut Vec<&'a Statement>) {
    for s in body {
        out.push(s);
        match s {
            Statement::If { then_body, else_body, .. } => {
                walk(then_body, out);
                if let Some(e) = else_body {
                    walk(e, out);
                }
            }
            Statement::While { body, .. } | Statement::For { body, .. } => walk(body, out),
            _ => {}
        }
    }
}

fn caps_in(program: &crush_cast::Program, func: &str) -> Vec<String> {
    let mut all = Vec::new();
    walk(&program.functions[func].body, &mut all);
    all.iter()
        .filter_map(|s| match s {
            Statement::ExprStmt { expr: Expression::CapabilityCall { name, .. }, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

fn lower_err(src: &str) -> String {
    format!("{:#}", crush_lang_zsh::zsh_to_cast(src).unwrap_err())
}

#[test]
fn test_zsh_echo_uses_io_print() {
    let cast = crush_lang_zsh::zsh_to_cast("echo hello\n").unwrap();
    assert!(caps_in(&cast, "main").contains(&"io.print".to_string()));
}

#[test]
fn test_zsh_echo_n_uses_io_write() {
    let cast = crush_lang_zsh::zsh_to_cast("echo -n hello\n").unwrap();
    assert!(caps_in(&cast, "main").contains(&"io.write".to_string()));
}

#[test]
fn test_zsh_function_def() {
    // Shell functions are mangled so they can't collide with Crush builtins.
    let cast = crush_lang_zsh::zsh_to_cast("greet() {\n    echo \"Hello\"\n}\ngreet\n").unwrap();
    assert!(cast.functions.contains_key("__shfn_greet"));
    assert!(caps_in(&cast, "__shfn_greet").contains(&"io.print".to_string()));
}

#[test]
fn test_zsh_until_is_not_while() {
    // `until` used to be lowered exactly like `while` (bash) — the two must differ.
    let w = serde_json::to_string(&crush_lang_zsh::zsh_to_cast("while false; do echo x; done\n").unwrap().functions["main"].body).unwrap();
    let u = serde_json::to_string(&crush_lang_zsh::zsh_to_cast("until false; do echo x; done\n").unwrap().functions["main"].body).unwrap();
    assert_ne!(w, u);
}

// Constructs that used to lower to something that ran and printed the wrong
// thing (or nothing) now fail with a located error (CRUSH-202).

#[test]
fn test_zsh_subshell_is_loud() {
    assert!(lower_err("(echo hello; echo world)\n").contains("unsupported: a subshell"));
}

#[test]
fn test_zsh_source_is_loud() {
    assert!(lower_err("source ./setup.sh\n").contains("unsupported: command `source`"));
}

#[test]
fn test_zsh_return_outside_function_is_loud() {
    assert!(lower_err("return 42\n").contains("`return` outside a function"));
}

#[test]
fn test_zsh_heredoc_is_loud() {
    assert!(lower_err("cat <<EOF\nhello world\nEOF\n").contains("unsupported"));
}

#[test]
fn test_zsh_error_names_the_line() {
    let e = lower_err("echo ok\necho ok\nls /tmp\n");
    assert!(e.contains("command `ls`") && e.contains("at line 3"), "{e}");
}

#[test]
fn test_zsh_never_assigned_variable_is_loud() {
    assert!(lower_err("echo $HOME\n").contains("never assigned"));
}
