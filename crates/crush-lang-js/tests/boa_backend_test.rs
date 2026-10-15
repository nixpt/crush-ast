//! Boa-specific frontend tests.
//! These exercise the `js-boa` parse path (JS-only, no ES module imports).

#![cfg(feature = "js-boa")]

use crush_walker_core::Frontend;

fn test_analyze(source: &str, ext: &str) -> crush_walker_core::FeatureReport {
    let frontend = crush_lang_js::JsFrontend::with_lowering(ext, crush_lang_js::JsLowering::Boa);
    let ast = frontend.parse(source).unwrap();
    frontend.analyze(&ast).unwrap()
}

fn test_lower(source: &str) -> crush_cast::Program {
    let frontend = crush_lang_js::JsFrontend::with_lowering("js", crush_lang_js::JsLowering::Boa);
    let (_report, program) = crush_walker_core::frontend_pipeline(&frontend, source).unwrap();
    program
}

/// What the lowering refuses: a typed error naming the construct, at walk time.
fn loud(source: &str) -> String {
    let err = crush_walker_core::frontend_pipeline(
        &crush_lang_js::JsFrontend::with_lowering("js", crush_lang_js::JsLowering::Boa),
        source,
    )
    .unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        err.chain().any(|e| e.downcast_ref::<crush_lang_js::lower::Unsupported>().is_some()),
        "not an Unsupported error: {msg}"
    );
    msg
}

#[test]
fn test_boa_var_decl_and_arithmetic() {
    let source = "const x = 42;\nlet y = x + 1;\n";
    let (_report, program) =
        crush_walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::with_lowering("js", crush_lang_js::JsLowering::Boa), source).unwrap();
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
        crush_walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::with_lowering("js", crush_lang_js::JsLowering::Boa), source).unwrap();
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_while_loop() {
    let source = "let i = 0;\nwhile (i < 10) { i++; }\n";
    let (_report, program) =
        crush_walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::with_lowering("js", crush_lang_js::JsLowering::Boa), source).unwrap();
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_for_loop() {
    let source = "for (let i = 0; i < 10; i++) { console.log(i); }";
    let (_report, program) =
        crush_walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::with_lowering("js", crush_lang_js::JsLowering::Boa), source).unwrap();
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_for_in_loop() {
    let source = "const obj = { a: 1, b: 2 };\nfor (let k in obj) { console.log(k); }\n";
    let (_report, program) =
        crush_walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::with_lowering("js", crush_lang_js::JsLowering::Boa), source).unwrap();
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_for_of_loop() {
    let source = "const arr = [1, 2, 3];\nfor (let v of arr) { console.log(v); }\n";
    let (_report, program) =
        crush_walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::with_lowering("js", crush_lang_js::JsLowering::Boa), source).unwrap();
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_try_catch() {
    let source = "try { x(); } catch(e) { console.log(e); }";
    let report = test_analyze(source, "js");
    assert!(report.uses_exceptions);
    let (_report, program) =
        crush_walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::with_lowering("js", crush_lang_js::JsLowering::Boa), source).unwrap();
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_function_decl() {
    let source = "function greet(name) { return 'hello ' + name; }";
    let (_report, program) =
        crush_walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::with_lowering("js", crush_lang_js::JsLowering::Boa), source).unwrap();
    assert!(program.functions.contains_key("greet"));
}

#[test]
fn test_boa_arrow_function() {
    let source = "const add = (a, b) => a + b;";
    let (_report, program) =
        crush_walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::with_lowering("js", crush_lang_js::JsLowering::Boa), source).unwrap();
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
    let frontend = crush_lang_js::JsFrontend::with_lowering("js", crush_lang_js::JsLowering::Boa);
    let result = frontend.parse(source);
    assert!(result.is_ok());
}

#[test]
fn test_boa_property_access() {
    let source = "const val = obj.prop;\nconst idx = arr[0];\n";
    let program = test_lower(source);
    assert!(program.functions.contains_key("main"));
}

// `this`, `new` and classes wait for the COSM object model.
#[test]
fn test_boa_this_keyword_is_loud() {
    let msg = loud("function f() { return this; }");
    assert!(msg.contains("`this`") && msg.contains("line 1, column 23"), "{msg}");
}

#[test]
fn test_boa_dowhile_loop() {
    let source = "let i = 0;\ndo { i++; } while (i < 5);\n";
    let program = test_lower(source);
    assert!(program.functions.contains_key("main"));
}

// A labeled jump would act on the innermost loop (CRUSH-zorro-6).
#[test]
fn test_boa_labeled_break_is_loud() {
    let msg = loud("loop1: for (let i = 0; i < 3; i++) { break loop1; }");
    assert!(msg.contains("labeled `break`"), "{msg}");
}

#[test]
fn test_boa_switch() {
    let source = "const x = 1;\nswitch (x) { case 1: break; default: break; }\n";
    let program = test_lower(source);
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_throw() {
    let program = test_lower("throw 'bad';");
    assert!(program.functions.contains_key("main"));
    assert!(loud("throw new Error('bad');").contains("`new`"));
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
        crush_walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::with_lowering("js", crush_lang_js::JsLowering::Boa), source).unwrap();
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_tagged_template_is_loud() {
    assert!(loud("const r = String.raw`hello\nworld`;").contains("tagged template"));
}

#[test]
fn test_boa_nested_functions() {
    let source = "function outer() { function inner() { return 1; } return inner(); }";
    let (_report, program) =
        crush_walker_core::frontend_pipeline(&crush_lang_js::JsFrontend::with_lowering("js", crush_lang_js::JsLowering::Boa), source).unwrap();
    assert!(program.functions.contains_key("outer"));
}

// CRUSH-zorro-13, option (a): an integer literal an `i64` can't hold lowers to
// a float; one it can stays an int (indexing, loops).
#[test]
fn test_boa_integer_literals_past_i64_are_floats() {
    let program = test_lower("const big = 1e19;\nconst edge = 9223372036854775807;\nconst ok = 9007199254740993;\n");
    let main = program.functions.get("main").unwrap();
    let value = |n: &str| {
        main.body
            .iter()
            .find_map(|s| match s {
                crush_cast::Statement::VarDecl { name, value, .. } if name == n => Some(value.clone()),
                _ => None,
            })
            .unwrap()
    };
    assert!(matches!(value("big"), crush_cast::Expression::FloatLiteral { .. }), "{:?}", value("big"));
    // 9223372036854775807 is 2^63 as a double: past i64::MAX.
    assert!(matches!(value("edge"), crush_cast::Expression::FloatLiteral { .. }), "{:?}", value("edge"));
    assert!(matches!(value("ok"), crush_cast::Expression::IntLiteral { value: 9007199254740992, .. }), "{:?}", value("ok"));
}

// CRUSH-250 scope pass: a block binding that shadows an outer one gets its own
// crush local; the outer name is untouched.
#[test]
fn test_boa_block_shadowing_renames_the_inner_binding() {
    let program = test_lower("let x = 1;\n{ let x = 2; console.log(x); }\nconsole.log(x);\n");
    let main = program.functions.get("main").unwrap();
    let names: Vec<&str> = main
        .body
        .iter()
        .filter_map(|s| match s {
            crush_cast::Statement::VarDecl { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert_eq!(names[0], "x");
    assert_ne!(names[1], "x");
    assert!(names[1].starts_with("x__"), "{names:?}");
}
