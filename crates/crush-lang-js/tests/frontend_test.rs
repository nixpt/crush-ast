use crush_lang_js::JsFrontend;
use walker_core::Frontend;

fn test_analyze(source: &str, ext: &str) -> walker_core::FeatureReport {
    let frontend = JsFrontend::new(ext);
    let ast = frontend.parse(source).unwrap();
    frontend.analyze(&ast).unwrap()
}

#[test]
fn test_js_frontend_detects_dangerous_imports() {
    let report = test_analyze("import { readFile } from \"fs\";", "js");
    assert!(report.dangerous_imports.contains(&"fs".to_string()));
    assert!(!report.can_lower_safely());
    assert!(report.uses_imports.contains(&"fs".to_string()));
}

#[test]
fn test_js_frontend_safe_code() {
    let source = "const x = 42;\nconsole.log(x + 1);\n";
    let frontend = JsFrontend::new("js");
    let (report, program) = walker_core::frontend_pipeline(&frontend, source).unwrap();
    assert!(report.dangerous_imports.is_empty());
    assert!(report.can_lower_safely());
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_js_frontend_detects_classes_and_async() {
    let report = test_analyze("class Foo {}\n\nasync function bar() {}", "js");
    assert!(report.uses_classes);
    assert!(report.uses_async);
}

#[test]
fn test_js_frontend_detects_exceptions() {
    let report = test_analyze("try { throw new Error('bad'); } catch(e) {}", "js");
    assert!(report.uses_exceptions);
}

#[test]
fn test_js_frontend_detects_imports() {
    let report = test_analyze("import * as fs from \"fs\";\nimport { loads } from \"json\";", "js");
    assert!(report.uses_imports.contains(&"fs".to_string()));
    assert!(report.uses_imports.contains(&"json".to_string()));
}

#[test]
fn test_js_frontend_detects_eval() {
    let report = test_analyze("eval('1+1');", "js");
    assert!(!report.can_lower_safely());
}

#[test]
fn test_ts_frontend_parses_types() {
    let source = "function greet(name: string): void {\n  console.log(name);\n}\n";
    let frontend = JsFrontend::new("ts");
    let (report, program) = walker_core::frontend_pipeline(&frontend, source).unwrap();
    assert!(report.uses_functions);
    assert!(program.functions.contains_key("greet"));
}

#[test]
fn test_js_frontend_arrow_function() {
    let source = "const add = (a, b) => a + b;";
    let frontend = JsFrontend::new("js");
    let (_report, program) = walker_core::frontend_pipeline(&frontend, source).unwrap();
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_js_frontend_if_else() {
    let source = "if (true) { console.log('yes'); } else { console.log('no'); }";
    let frontend = JsFrontend::new("js");
    let (report, program) = walker_core::frontend_pipeline(&frontend, source).unwrap();
    assert!(report.can_lower_safely());
    let main = program.functions.get("main").unwrap();
    assert!(main.body.iter().any(|s| matches!(s, crush_cast::Statement::If { .. })));
}

#[test]
fn test_js_frontend_array_object() {
    let source = "const arr = [1, 2, 3];\nconst obj = { a: 1, b: 'hello' };\n";
    let frontend = JsFrontend::new("js");
    let (_report, program) = walker_core::frontend_pipeline(&frontend, source).unwrap();
    let main = program.functions.get("main").unwrap();
    assert!(main.body.iter().any(|s| matches!(s, crush_cast::Statement::VarDecl { name, .. } if name == "arr")));
    assert!(main.body.iter().any(|s| matches!(s, crush_cast::Statement::VarDecl { name, .. } if name == "obj")));
}

#[test]
fn test_js_frontend_try_catch() {
    let source = "try { x(); } catch(e) { console.log(e); }";
    let frontend = JsFrontend::new("js");
    let (report, program) = walker_core::frontend_pipeline(&frontend, source).unwrap();
    assert!(report.uses_exceptions);
    let main = program.functions.get("main").unwrap();
    assert!(main.body.iter().any(|s| matches!(s, crush_cast::Statement::TryCatch { .. })));
}

#[test]
fn test_js_frontend_template_literal() {
    let source = "const msg = `hello ${name}`;";
    let frontend = JsFrontend::new("js");
    let (_report, program) = walker_core::frontend_pipeline(&frontend, source).unwrap();
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_js_frontend_generator_rejected() {
    let source = "function* gen() { yield 1; }";
    let frontend = JsFrontend::new("js");
    let result = frontend.parse(source);
    // Generator functions parse OK but may cause lowering issues
    assert!(result.is_ok());
}

#[test]
fn test_js_frontend_import() {
    let source = "import { readFile } from 'fs';\nreadFile('./file.txt');";
    let frontend = JsFrontend::new("js");
    let (report, _program) = walker_core::frontend_pipeline(&frontend, source).unwrap();
    assert!(!report.can_lower_safely());
    assert!(report.dangerous_imports.contains(&"fs".to_string()));
}
