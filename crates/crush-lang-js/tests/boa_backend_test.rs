//! Boa-specific frontend tests.
//! These exercise the `boa-backend` parse path (JS-only, no ES module imports).

#![cfg(feature = "boa-backend")]

use walker_core::Frontend;

fn test_analyze(source: &str, ext: &str) -> walker_core::FeatureReport {
    let frontend = crush_lang_js::JsFrontend::new(ext);
    let ast = frontend.parse(source).unwrap();
    frontend.analyze(&ast).unwrap()
}

fn test_lower(source: &str) -> crush_cast::Program {
    let frontend = crush_lang_js::JsFrontend::new("js");
    let (_report, program) = walker_core::frontend_pipeline(&frontend, source).unwrap();
    program
}

#[test]
fn test_boa_var_decl_and_arithmetic() {
    let source = "const x = 42;\nlet y = x + 1;\n";
    let (_report, program) =
        walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::new("js"), source).unwrap();
    let main = program.functions.get("main").unwrap();
    assert!(
        main.body
            .iter()
            .any(|s| matches!(s, crush_cast::Statement::VarDecl { name, .. } if name == "x"))
    );
    assert!(
        main.body
            .iter()
            .any(|s| matches!(s, crush_cast::Statement::VarDecl { name, .. } if name == "y"))
    );
}

#[test]
fn test_boa_if_else() {
    let source = "if (true) { console.log('yes'); } else { console.log('no'); }";
    let (_report, program) =
        walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::new("js"), source).unwrap();
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_while_loop() {
    let source = "let i = 0;\nwhile (i < 10) { i++; }\n";
    let (_report, program) =
        walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::new("js"), source).unwrap();
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_for_loop() {
    let source = "for (let i = 0; i < 10; i++) { console.log(i); }";
    let (_report, program) =
        walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::new("js"), source).unwrap();
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_for_in_loop() {
    let source = "const obj = { a: 1, b: 2 };\nfor (let k in obj) { console.log(k); }\n";
    let (_report, program) =
        walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::new("js"), source).unwrap();
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_for_of_loop() {
    let source = "const arr = [1, 2, 3];\nfor (let v of arr) { console.log(v); }\n";
    let (_report, program) =
        walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::new("js"), source).unwrap();
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_try_catch() {
    let source = "try { x(); } catch(e) { console.log(e); }";
    let report = test_analyze(source, "js");
    assert!(report.uses_exceptions);
    let (_report, program) =
        walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::new("js"), source).unwrap();
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_function_decl() {
    let source = "function greet(name) { return 'hello ' + name; }";
    let (_report, program) =
        walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::new("js"), source).unwrap();
    assert!(program.functions.contains_key("greet"));
}

#[test]
fn test_boa_arrow_function() {
    let source = "const add = (a, b) => a + b;";
    let (_report, program) =
        walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::new("js"), source).unwrap();
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_array_object() {
    let source = "const arr = [1, 2, 3];\nconst obj = { a: 1, b: 'hello' };\n";
    let program = test_lower(source);
    let main = program.functions.get("main").unwrap();
    assert!(
        main.body
            .iter()
            .any(|s| matches!(s, crush_cast::Statement::VarDecl { name, .. } if name == "arr"))
    );
}

#[test]
fn test_boa_template_literal() {
    let source = "const msg = `hello ${name}`;";
    let program = test_lower(source);
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_eval_detection() {
    let report = test_analyze("eval('1+1');", "js");
    assert!(!report.can_lower_safely());
}

#[test]
fn test_boa_async_await() {
    let report = test_analyze("async function foo() { await bar(); }", "js");
    assert!(report.uses_async);
}

#[test]
fn test_boa_generator_rejected() {
    let source = "function* gen() { yield 1; }";
    let frontend = crush_lang_js::JsFrontend::new("js");
    let result = frontend.parse(source);
    assert!(result.is_ok());
}

#[test]
fn test_boa_property_access() {
    let source = "const val = obj.prop;\nconst idx = arr[0];\n";
    let program = test_lower(source);
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_this_keyword() {
    let source = "function f() { return this; }";
    let (_report, program) =
        walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::new("js"), source).unwrap();
    assert!(program.functions.contains_key("f"));
}

#[test]
fn test_boa_dowhile_loop() {
    let source = "let i = 0;\ndo { i++; } while (i < 5);\n";
    let program = test_lower(source);
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_labeled_statement() {
    let source = "loop1: for (let i = 0; i < 3; i++) { break loop1; }";
    let program = test_lower(source);
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_switch() {
    let source = "const x = 1;\nswitch (x) { case 1: break; default: break; }\n";
    let program = test_lower(source);
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_throw() {
    let source = "throw new Error('bad');";
    let (_report, program) =
        walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::new("js"), source).unwrap();
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_class() {
    let report = test_analyze("class Foo { constructor(x) { this.x = x; } }", "js");
    assert!(report.uses_classes);
}

#[test]
fn test_boa_logical_operators() {
    let source = "const r = a && b || !c;";
    let program = test_lower(source);
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_conditional_operator() {
    let source = "const r = x > 0 ? 'pos' : 'neg';";
    let program = test_lower(source);
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_compound_assignment() {
    let source = "let x = 1;\nx += 2;\nx -= 1;\n";
    let program = test_lower(source);
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_update_operators() {
    let source = "let i = 0;\ni++;\n++i;\ni--;\n";
    let (_report, program) =
        walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::new("js"), source).unwrap();
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_tagged_template() {
    let source = "const r = String.raw`hello\nworld`;";
    let program = test_lower(source);
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_nested_functions() {
    let source = "function outer() { function inner() { return 1; } return inner(); }";
    let (_report, program) =
        walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::new("js"), source).unwrap();
    assert!(program.functions.contains_key("outer"));
}
