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
    // `Error` isn't a class this module declares: `new` stays loud.
    assert!(loud("throw new Error('bad');").contains("`new Error`"));
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
    assert!(o.contains("`.f()`") && o.contains("CRUSH-277"), "{o}");
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

// ── CRUSH-277 S1: classes lower to COSM ────────────────────────────────────

fn only_kind(source: &str) -> (crush_cast::Program, crush_cosm::KindDecl) {
    let program = test_lower(source);
    let cosm = program.cosm.as_ref().expect("a class emits COSM");
    assert_eq!(cosm.kinds.len(), 1, "one class, one kind: {cosm:?}");
    (program.clone(), cosm.kinds[0].clone())
}

#[test]
fn test_boa_class_emits_a_js_kind() {
    let (program, kind) = only_kind(
        "class Counter { constructor(x) { this.x = x; } inc() { return this.x; } }",
    );
    assert_eq!(kind.name, "Counter");
    assert_eq!(kind.home_lang, "javascript");
    assert!(matches!(kind.storage, crush_cosm::Storage::OpenDict));
    assert!(matches!(kind.semantics, crush_cosm::Semantics::Reference));
    assert!(matches!(kind.resolution, crush_cosm::Resolution::PrototypeChain));
    assert!(matches!(kind.receiver, crush_cosm::Receiver::DynamicThis));
    assert!(matches!(kind.equality, crush_cosm::Equality::Identity));
    assert!(
        matches!(&kind.construct, crush_cosm::Construct::JsNew { ctor_fn } if ctor_fn.as_str() == "Counter::constructor"),
        "construct: {:?}",
        kind.construct
    );
    let inc = kind.members.iter().find(|m| m.name == "inc").expect("method member");
    assert!(
        matches!(&inc.member, crush_cosm::Member::Method { func, receiver: None } if func.as_str() == "Counter::inc"),
        "member: {:?}",
        inc.member
    );
    // The lifted functions take the receiver as a first parameter `this`.
    let ctor = &program.functions["Counter::constructor"];
    let names: Vec<&str> = ctor.params.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["this", "x"]);
    assert_eq!(program.functions["Counter::inc"].params[0].0, "this");
}

#[test]
fn test_boa_class_without_constructor_gets_an_empty_one() {
    let (program, kind) = only_kind("class Empty { m() { return 1; } }");
    assert!(
        matches!(&kind.construct, crush_cosm::Construct::JsNew { ctor_fn } if ctor_fn.as_str() == "Empty::constructor")
    );
    let ctor = &program.functions["Empty::constructor"];
    assert_eq!(ctor.params.len(), 1);
    assert!(ctor.body.is_empty());
}

#[test]
fn test_boa_extends_records_the_parent_kind() {
    let program = test_lower(
        "class Base { m() { return 1; } } class Child extends Base { constructor(x) { this.x = x; } }",
    );
    let cosm = program.cosm.expect("classes emit COSM");
    assert_eq!(cosm.kinds.len(), 2);
    let child = cosm.kinds.iter().find(|k| k.name == "Child").unwrap();
    assert_eq!(child.parents.len(), 1);
    assert_eq!(child.parents[0].as_str(), "Base");
}

#[test]
fn test_boa_exported_class_emits_kind_and_export() {
    let program = test_lower("export class Foo { m() { return 1; } }");
    let cosm = program.cosm.expect("exported class emits COSM");
    assert_eq!(cosm.kinds[0].name, "Foo");
    let main = &program.functions["main"];
    assert!(
        main.body.iter().any(|s| matches!(s, crush_cast::Statement::Export { name, .. } if name == "Foo")),
        "export statement missing: {:?}",
        main.body
    );
}

#[test]
fn test_boa_named_class_expression_emits_a_kind() {
    let (program, kind) = only_kind("const K = class K { m() { return 1; } };");
    assert_eq!(kind.name, "K");
    let main = &program.functions["main"];
    assert!(
        main.body.iter().any(|s| matches!(s, crush_cast::Statement::VarDecl { name, .. } if name == "K")),
        "binding missing: {:?}",
        main.body
    );
}

#[test]
fn test_boa_binding_named_class_expression_uses_the_binding_name() {
    // `const K = class { … }`: boa infers the binding as the class's name.
    let (_, kind) = only_kind("const K = class { m() { return 1; } };");
    assert_eq!(kind.name, "K");
}

#[test]
fn test_boa_accessor_pair_merges_into_one_member() {
    let (_, kind) = only_kind(
        "class Box { get v() { return 1; } set v(x) { this.x = x; } }",
    );
    let accessors: Vec<_> = kind.members.iter().filter(|m| m.name == "v").collect();
    assert_eq!(accessors.len(), 1, "get/set merge: {:?}", kind.members);
    match &accessors[0].member {
        crush_cosm::Member::Accessor { get, set } => {
            assert_eq!(get.as_ref().unwrap().as_str(), "Box::get::v");
            assert_eq!(set.as_ref().unwrap().as_str(), "Box::set::v");
        }
        other => panic!("expected an accessor, got {other:?}"),
    }
}

#[test]
fn test_boa_static_method_is_a_static_member() {
    let (_, kind) = only_kind("class Util { static twice(x) { return x; } }");
    let m = kind.members.iter().find(|m| m.name == "twice").expect("static member");
    assert!(
        matches!(&m.member, crush_cosm::Member::Static { value: crush_cosm::StaticValue::Fn(f) } if f.as_str() == "Util::static::twice"),
        "member: {:?}",
        m.member
    );
}

#[test]
fn test_boa_class_program_compiles() {
    // CRUSH-277 S2: prototype chains run on the COSM VMs, so a class
    // program compiles end to end. Running it does nothing yet — the
    // class name becomes a value, and `new` constructs, with S3.
    let program = test_lower("class Counter { inc() { return 1; } }");
    crush_frontend::compile_cast(&program).unwrap();
}

#[test]
fn test_boa_class_loud_edges() {
    assert!(loud("console.log(class { m() { return 1; } });").contains("anonymous class expression"), "anonymous class expression");
    assert!(loud("class A { #m() { return 1; } }").contains("private class method"), "private method");
    assert!(loud("class A { static { } }").contains("static {"), "static block");
    assert!(loud("class A extends B { }").contains("isn't declared in this module"), "derived without ctor, parent undeclared");
    // `super.m()` resolves from the parent up; these shapes stay loud.
    assert!(loud("class A { m() { return super.m(); } }").contains("no ancestor"), "super without parent");
    assert!(loud("class B { } class A extends B { m() { return super.m(); } }").contains("no ancestor"), "super of nothing");
    assert!(loud("class B { x = 1; } class A extends B { m() { return super.x; } }").contains("`super.x` read"), "super.x read");
    // `new` of a name that isn't a class declared in this module.
    assert!(loud("const s = new Set();").contains("`new Set`"), "host constructor");
    assert!(loud("class A { } A();").contains("without `new`"), "class called bare");
    assert!(loud("class A extends ns.Base { constructor() { } }").contains("anything but a name"), "expression superclass");
    assert!(loud("class A { async m() { return 1; } }").contains("async method"), "async method");
    assert!(loud("class A { [k]() { return 1; } }").contains("computed member name"), "computed name");
    // `this` is bound only directly inside a method.
    assert!(loud("class A { m() { function g() { return this; } return g(); } }").contains("`this` outside a class method"), "nested fn this");
    assert!(loud("class A { m() { const g = () => this; return g(); } }").contains("`this` outside a class method"), "arrow this");
}

/// Compile a lowered JS program all the way to a loaded CVM1 program —
/// CASM JSON, `casm_to_vm`, the CVM1 blob — and run it on the scheduler VM.
fn run_js(source: &str) -> String {
    let program = test_lower(source);
    let casm = crush_frontend::compile_cast(&program).unwrap();
    let json = casm.serialize(casm::Format::Json).unwrap();
    let casm = casm::Program::deserialize(&json, casm::Format::Json).unwrap();
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm).unwrap();
    let loaded = crush_vm::Program::from_blob(&vm.to_blob()).unwrap();
    crush_vm::run(&loaded, &crush_vm::Quotas::default())
        .unwrap()
        .output
}

/// Like `run_js`, but the run must fail; returns the error's text.
fn run_js_err(source: &str) -> String {
    let program = test_lower(source);
    let casm = crush_frontend::compile_cast(&program).unwrap();
    let json = casm.serialize(casm::Format::Json).unwrap();
    let casm = casm::Program::deserialize(&json, casm::Format::Json).unwrap();
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm).unwrap();
    let loaded = crush_vm::Program::from_blob(&vm.to_blob()).unwrap();
    crush_vm::run(&loaded, &crush_vm::Quotas::default())
        .unwrap_err()
        .to_string()
}

// CRUSH-285 review #1: accessors run as ordinary calls on the running
// thread — a self-reading getter hits the call-depth quota (a VmError,
// never a native stack overflow), and a getter's throw is caught by the
// program's own `try`/`catch`.
#[test]
fn test_boa_accessor_recursion_and_throw() {
    let err = run_js_err(
        "class C { get x() { return this.x; } } console.log(new C().x);",
    );
    assert!(err.contains("call depth"), "recursive getter: {err}");
    // A getter that never returns burns the program's own step quota.
    let err = run_js_err(
        "class C { get x() { for (;;) { } } } console.log(new C().x);",
    );
    assert!(err.contains("instruction quota"), "looping getter: {err}");
    let source = r#"
class C { get x() { throw "boom"; } }
const c = new C();
try {
  console.log(c.x);
} catch (e) {
  console.log("caught");
}
console.log("after");
"#;
    assert_eq!(run_js(source), "caught\nafter\n");
}

// CRUSH-285 review #2: an instance bound in a block (a renamed, block
// -scoped binding) calls through its own binding, not an outer one of
// the same JS name.
#[test]
fn test_boa_instance_in_block_calls_own_binding() {
    let source = r#"
class C {
  constructor(x) { this.x = x; }
  get() { return this.x; }
}
const c = new C(1);
for (let i = 0; i < 3; i++) {
  const c = new C(i);
  console.log(c.get());
}
console.log(c.get());
"#;
    assert_eq!(run_js(source), "0\n1\n2\n1\n");
    // No outer binding at all: still a member call, not a host cap call.
    let source = r#"
class C {
  constructor(x) { this.x = x; }
  get() { return this.x; }
}
for (let i = 0; i < 2; i++) {
  const c = new C(i);
  console.log(c.get());
}
"#;
    assert_eq!(run_js(source), "0\n1\n");
}

// CRUSH-285 review #4: calling a method the class doesn't have is a
// compile-time refusal, never an automatically granted host capability
// named after the method (the pre-review fallback emitted exactly
// that, permissions included).
#[test]
fn test_boa_missing_method_is_compile_error() {
    let program =
        test_lower("class C { m() { return 1; } } const c = new C(); console.log(c.nope());");
    let err = crush_frontend::compile_cast(&program)
        .err()
        .expect("a missing method must not compile");
    assert!(err.to_string().contains("no method `nope`"), "{err}");
    // A field holding a value, called through `this.`, is the same refusal.
    let program = test_lower(
        "class C { constructor() { this.f = 1; } m() { return this.f(2); } } console.log(new C().m());",
    );
    let err = crush_frontend::compile_cast(&program)
        .err()
        .expect("calling a field must not compile");
    assert!(err.to_string().contains("no method `f`"), "{err}");
}

// CRUSH-285 review #3: statics written at run time are read through
// the inheritance chain until a nearer class writes its own, a getter
// sees the write (accessors run in the same run, CRUSH-285 #1), and
// statics are not members of instances.
#[test]
fn test_boa_statics_runtime_writes_and_inheritance() {
    let source = r#"
class A { static n = 0; }
class B extends A { }
A.n = 5;
console.log(B.n);
B.n = 7;
console.log(A.n);
console.log(B.n);
class C {
  static n = 0;
  get t() { return C.n; }
}
C.n = 42;
console.log(new C().t);
"#;
    assert_eq!(run_js(source), "5\n5\n7\n42\n");
    // Calling a static through an instance is refused at run time when
    // the call compiles as a member call (another class has an instance
    // method of the name), and at compile time otherwise (CRUSH-285 #4).
    let err = run_js_err(
        "class C { static s() { return 1; } } class D { s() { return 2; } } const c = new C(); console.log(c.s());",
    );
    assert!(err.contains("static method"), "static via instance: {err}");
    // A static's value is not readable on an instance either: it reads
    // as a missing member (crush's null, JS's undefined) — never as the
    // static's value.
    assert_eq!(
        run_js("class C { static n = 3; } const c = new C(); console.log(c.n);"),
        "null\n"
    );
}

// CRUSH-285 review #5: a write inside `new` arguments is a mutation
// the optimizer must see (it folded `n` to its pre-construction value).
#[test]
fn test_boa_construct_args_mutation_seen_by_optimizer() {
    let source = r#"
class C { constructor(x) { this.x = x; } }
class D {
  make() {
    let n = 0;
    const a = new C(n++);
    console.log(n);
    return a;
  }
}
new D().make();
"#;
    assert_eq!(run_js(source), "1\n");
}

// CRUSH-285 review #6: a class field *defines* its own property — a
// parent setter of the same name must not run for the initialiser.
#[test]
fn test_boa_field_defines_not_sets() {
    let source = r#"
class Base {
  set v(x) { this.hidden = x * 100; }
  get v() { return this.hidden; }
}
class Sub extends Base {
  v = 5;
  constructor() { super(); }
}
const s = new Sub();
console.log(s.v);
"#;
    // JS: the field is an own data property (5); through the setter
    // this would read back 500.
    assert_eq!(run_js(source), "5\n");
}

// CRUSH-285 review #6: a field initialiser must see the outer binding
// of a name the constructor also declares (refused, not captured), and
// a branched `super(...)` must not run the fields before the parent
// constructor (refused, not misplaced).
#[test]
fn test_boa_field_init_capture_and_branched_super_refused() {
    assert!(
        loud("const n = 5; class A { v = n; constructor(n) { this.n = n; } }")
            .contains("also declares as a parameter"),
        "field init shadowed by ctor param"
    );
    // Without a collision the initialiser runs (reading a top-level
    // function here; module-level `const` reads from members are the
    // separate CRUSH-110 wall).
    let source = r#"
function base() { return 5; }
class A {
  v = base();
  constructor(x) { this.x = x; }
}
console.log(new A(1).v);
"#;
    assert_eq!(run_js(source), "5\n");
    assert!(
        loud("class B { constructor() { this.y = 1; } } class A extends B { v = 2; constructor(c) { if (c) { super(); } else { super(); } } }")
            .contains("inside a branch"),
        "branched super with fields"
    );
}

// CRUSH-277 S3: `new`, `this` and method calls, end to end.
#[test]
fn test_boa_new_this_and_method_calls_run() {
    let source = r#"
class Counter {
  constructor(start) { this.x = start; }
  inc() { this.x = this.x + 1; }
  bump(by) { this.x = this.x + by; }
  get() { return this.x; }
}
const c = new Counter(41);
c.inc();
c.bump(0);
console.log(c.get());
"#;
    assert_eq!(run_js(source), "42\n");
}

#[test]
fn test_boa_super_and_inherited_method_run() {
    let source = r#"
class Box {
  constructor(x) { this.x = x; }
  get() { return this.x; }
}
class BigBox extends Box {
  constructor(x) { super(x); }
}
const b = new BigBox(7);
console.log(b.get());
"#;
    assert_eq!(run_js(source), "7\n");
}

// A derived class without a constructor forwards its arguments to `super`.
#[test]
fn test_boa_derived_default_ctor_forwards_run() {
    let source = r#"
class Box {
  constructor(x) { this.x = x; }
  get() { return this.x; }
}
class BigBox extends Box { }
const b = new BigBox(9);
console.log(b.get());
"#;
    assert_eq!(run_js(source), "9\n");
}

#[test]
fn test_boa_static_method_runs() {
    let source = r#"
class Util { static twice(x) { return x * 2; } }
console.log(Util.twice(21));
"#;
    assert_eq!(run_js(source), "42\n");
    // The lifted static takes its plain parameters only (the VM calls a
    // static with no receiver) — no implicit `this` parameter.
    let program = test_lower("class Util { static twice(x) { return x * 2; } }");
    let params = &program.functions["Util::static::twice"].params;
    assert_eq!(params.len(), 1, "{params:?}");
    assert_eq!(params[0].0, "x");
}

// CRUSH-277 S4: getters and setters run end to end.
#[test]
fn test_boa_accessors_run() {
    let source = r#"
class Meter {
  constructor(x) { this.x = x; }
  get doubled() { return this.x * 2; }
  set doubled(v) { this.x = v / 2; }
}
const m = new Meter(21);
console.log(m.doubled);
m.doubled = 100;
console.log(m.doubled);
"#;
    assert_eq!(run_js(source), "42\n100\n");
}

// CRUSH-277 S5: class fields initialise on the instance — at the top
// of a base constructor's body, after `super(...)` in a derived one.
#[test]
fn test_boa_class_fields_run() {
    let source = r#"
class Counter {
  step = 2;
  total = 0;
  constructor(start) { this.x = start; }
  bump() { this.total = this.total + this.step; return this.x + this.total; }
}
const c = new Counter(40);
console.log(c.bump());
// A derived class's fields run after super(): the body sees them.
class Fast extends Counter {
  step = 10;
  constructor() { super(0); }
}
const f = new Fast();
console.log(f.bump());
"#;
    // c: total 2 -> 40+2; f: x=0, total 10 -> 10.
    assert_eq!(run_js(source), "42\n10\n");
}

#[test]
fn test_boa_super_method_calls_run() {
    let source = r#"
class A {
  hi() { return 'A'; }
}
class B extends A {
  hi() { return 'B' + super.hi(); }
}
console.log(new B().hi());
console.log(new A().hi());
"#;
    assert_eq!(run_js(source), "BA\nA\n");
}

// CRUSH-277 S5: static constants read through the class, static methods
// run through it, and `C.x = v` writes a class-level value.
#[test]
fn test_boa_statics_run() {
    let source = r#"
class Util {
  static factor = 3;
  static scale(x) { return x * Util.factor; }
}
console.log(Util.scale(14));
Util.factor = 2;
console.log(Util.scale(14));
"#;
    assert_eq!(run_js(source), "42\n28\n");
}

#[test]
fn test_boa_chained_calls_run() {
    let source = r#"
class P {
  constructor(x) { this.x = x; }
  get2() { return this.x * 2; }
  spawn() { return new P(this.x + 1); }
}
console.log(new P(4).get2());
console.log(new P(1).spawn().get2());
"#;
    assert_eq!(run_js(source), "8\n4\n");
}

// A class name used as a value loads its kind (D8).
#[test]
fn test_boa_class_as_value_loads_kind() {
    let program = test_lower("class Counter { inc() { return 1; } }\nconst K = Counter;");
    let casm = crush_frontend::compile_cast(&program).unwrap();
    let main = casm
        .functions
        .iter()
        .find(|(name, _)| *name == "main")
        .map(|(_, f)| f)
        .expect("main");
    assert!(
        main.body.iter().any(|i| i.op == "load_kind" && i.args["kind"] == "Counter"),
        "load_kind Counter in main: {:?}",
        main.body
    );
    // The class-expression binding is the kind name itself: `new C()` works.
    let source = "const C = class { get() { return 3; } };\nconst i = new C();\nconsole.log(i.get());";
    assert_eq!(run_js(source), "3\n");
}

#[test]
fn test_boa_classless_program_has_no_cosm() {
    let program = test_lower("const x = 1;\nconsole.log(x);");
    assert_eq!(program.cosm, None);
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
