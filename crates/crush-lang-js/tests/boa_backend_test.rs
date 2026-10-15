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
    let source = "function x() { throw 1; }\ntry { x(); } catch(e) { console.log(e); }";
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
    let source = "const name = 'x';\nconst msg = `hello ${name}`;";
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
    let source = "const obj = {prop: 1};\nconst arr = [2];\nconst val = obj.prop;\nconst idx = arr[0];\n";
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
    let source = "let a = 1, b = 0, c = 1;\nconst r = a && b || !c;";
    let program = test_lower(source);
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_boa_conditional_operator() {
    let source = "const x = 1;\nconst r = x > 0 ? 'pos' : 'neg';";
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

// A nested function that uses no binding of an enclosing function is lifted to
// a crush function of its own (direct calls; AOT compiles them); one that does
// stays a closure.
#[test]
fn test_boa_capture_free_nested_functions_are_lifted() {
    let program = test_lower(
        "function outer(k) {\n  function sq(x) { return x * x; }\n  function addK(n) { return n + k; }\n  return sq(addK(1));\n}\nconsole.log(outer(2));\n",
    );
    let lifted: Vec<&String> = program.functions.keys().filter(|n| n.starts_with("sq__")).collect();
    assert_eq!(lifted.len(), 1, "{:?}", program.functions.keys().collect::<Vec<_>>());
    assert!(!program.functions.keys().any(|n| n.starts_with("addK")));
    let outer = &program.functions["outer"];
    assert!(
        outer.body.iter().any(|s| matches!(
            s,
            crush_cast::Statement::VarDecl { name, value: crush_cast::Expression::Lambda { .. }, .. } if name == "addK"
        )),
        "addK captures `k`, so it stays a closure"
    );
}

// CRUSH-250 step 3: calls go through the builtin/method table. What isn't a
// user binding or a row fails at walk time, naming what isn't supported.
#[test]
fn test_boa_builtin_table_refusals() {
    // A planned row names the ticket that adds it.
    assert!(loud("console.log(parseInt('4'));").contains("`parseInt()` (CRUSH-zorro-17"));
    assert!(loud("let s = 'a';\nconsole.log(s.toUpperCase());").contains("`.toUpperCase()` (CRUSH-zorro-14"));
    // Not a row and not defined: refused here, not as an unknown function later.
    assert!(loud("nope(1);").contains("calling `nope`, which is not defined"));
    assert!(loud("console.log(Math.cbrt(8));").contains("`Math.cbrt()`"));
    let o = loud("const o = {f: 1};\no.f();");
    assert!(o.contains("`.f()`") && o.contains("COSM"), "{o}");
    // Arity is checked; `push`'s result (the array, not JS's new length) can't be used.
    assert!(loud("console.log(Math.max(1, 2, 3));").contains("with 3 argument(s)"));
    // Several values are expanded in statement position only.
    assert!(loud("const a = [];\nconsole.log(a.push(1, 2));").contains("using the result of `.push()` with 2 arguments"));
}

// A user binding shadows a builtin name: it's called, not looked up.
#[test]
fn test_boa_user_bindings_shadow_builtins() {
    let program = test_lower("function parseInt(s) { return 7; }\nconst Math = {floor: 1};\nconsole.log(parseInt('1'));\n");
    assert!(program.functions.contains_key("parseInt"));
    let msg = loud("const Math = {floor: (x) => x};\nconsole.log(Math.floor(2));");
    assert!(msg.contains("`.floor()`"), "a user `Math` is a receiver, not the namespace: {msg}");
}

// A name no scope binds is refused at walk time (never "uninitialised slot"
// at run time, or `null` on AOT).
#[test]
fn test_boa_unbound_names_are_loud() {
    assert!(loud("console.log(undefined);").contains("`undefined` as a value (CRUSH-zorro-12"));
    assert!(loud("console.log(Math.PI);").contains("`Math.PI` (CRUSH-zorro-17"));
    assert!(loud("console.log(nothing);").contains("`nothing`, which is not defined"));
}

// CRUSH-250 step 4: a top-level function that uses a script variable is a
// closure in `main` over it; one that doesn't stays a crush function of its
// own (calls are direct; AOT compiles it).
#[test]
fn test_boa_top_level_functions_capture_script_variables() {
    let program = test_lower(
        "let total = 3;\nfunction f() { return total + g(); }\nfunction g() { return 1; }\nconsole.log(f());\n",
    );
    assert!(program.functions.contains_key("g"), "g captures nothing");
    assert!(!program.functions.contains_key("f"), "f captures `total`");
    let main = &program.functions["main"];
    assert!(main.body.iter().any(|s| matches!(
        s,
        crush_cast::Statement::VarDecl { name, value: crush_cast::Expression::Lambda { .. }, .. } if name == "f"
    )));
}

// `push`: several values in statement position become hidden locals (all
// evaluated first, as in JS) and one `push` each; as a value, `push` is
// `len` of the returned array (JS's new length).
#[test]
fn test_boa_push_forms() {
    let program = test_lower("const xs = [];\nxs.push(1, 2, 3);\nconst n = xs.push(4);\n");
    let main = &program.functions["main"];
    let pushes = main
        .body
        .iter()
        .filter(|s| matches!(s, crush_cast::Statement::ExprStmt {
            expr: crush_cast::Expression::Call { function, args, .. }, ..
        } if function == "xs.push" && args.len() == 1))
        .count();
    assert_eq!(pushes, 3, "{:#?}", main.body);
    let temps = main
        .body
        .iter()
        .filter(|s| matches!(s, crush_cast::Statement::VarDecl { name, .. } if name.starts_with("__push_")))
        .count();
    assert_eq!(temps, 3);
    assert!(main.body.iter().any(|s| matches!(s, crush_cast::Statement::VarDecl {
        name, value: crush_cast::Expression::Call { function, .. }, ..
    } if name == "n" && function == "len")));
}

// CRUSH-zorro-18: a `function` declaration's calls are adapted to its
// parameters. What can't be adapted is refused at walk time.
#[test]
fn test_boa_parameter_adaptation_refusals() {
    // A missing plain argument is `undefined` (CRUSH-zorro-12).
    let msg = loud("function f(a, b) { return a; }\nconsole.log(f(1));");
    assert!(msg.contains("calling `f` without its argument 2"), "{msg}");
    // An extra argument with side effects (pure ones are dropped).
    let msg = loud("let n = 0;\nfunction f(a) { return a; }\nconsole.log(f(1, n++));");
    assert!(msg.contains("extra argument that has side effects"), "{msg}");
    // Its hidden arguments come from direct calls only.
    let msg = loud("function f(a = 1) { return a; }\nconst g = f;\nconsole.log(g());");
    assert!(msg.contains("using `f` as a value"), "{msg}");
    // Arrow functions and function expressions can be called from anywhere.
    assert!(loud("const f = (a = 1) => a;\nconsole.log(f());").contains("outside a `function` declaration"));
    assert!(loud("const f = (...xs) => xs;\nconsole.log(f());").contains("outside a `function` declaration"));
}

// The ABI: defaults take a hidden "given" flag after the parameters; a rest
// parameter takes an array.
#[test]
fn test_boa_parameter_abi() {
    let program = test_lower("function f(a, b = a, ...rest) { return rest; }\nconsole.log(f(1), f(1, 2, 3, 4));\n");
    let f = &program.functions["f"];
    let names: Vec<&str> = f.params.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names.len(), 4, "{names:?}");
    assert_eq!(&names[..3], ["a", "b", "rest"]);
    assert!(names[3].starts_with("__has_b_"), "{names:?}");
}
