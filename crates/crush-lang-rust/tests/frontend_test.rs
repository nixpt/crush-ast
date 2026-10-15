use crush_lang_rust::RustFrontend;
use crush_walker_core_no_tree_sitter::Frontend;

fn test_analyze(source: &str) -> crush_walker_core_no_tree_sitter::FeatureReport {
    let frontend = RustFrontend;
    let ast = frontend.parse(source).unwrap();
    frontend.analyze(&ast).unwrap()
}

#[test]
fn test_rust_frontend_detects_functions() {
    let report = test_analyze("fn hello() { println!(\"world\"); }");
    assert!(report.uses_functions);
    assert_eq!(report.estimated_complexity, 1);
}

#[test]
fn test_rust_frontend_detects_classes() {
    let report = test_analyze("struct Point { x: i32, y: i32 }");
    assert!(report.uses_classes);
    assert!(!report.uses_functions);
}

#[test]
fn test_rust_frontend_detects_imports() {
    let report = test_analyze("use std::io;");
    assert!(!report.uses_imports.is_empty());
}

#[test]
fn test_rust_frontend_detects_ffi() {
    let report = test_analyze("extern \"C\" { fn abs(x: i32) -> i32; }");
    assert!(report.uses_ffi);
}

#[test]
fn test_rust_frontend_pipeline_run() {
    let source = "fn main() { let x = 42; println!(\"value: {}\", x); }";
    let frontend = RustFrontend;
    let (report, program) =
        crush_walker_core_no_tree_sitter::frontend_pipeline(&frontend, source).unwrap();
    assert!(report.uses_functions);
    assert!(program.functions.contains_key("main"));
}

// A destructuring closure parameter used to become a placeholder `_param`, so
// the body's names were free variables (captured from an enclosing scope if
// one had them: silently wrong). It is refused until destructuring exists.
#[test]
fn destructuring_closure_parameters_fail_loudly() {
    let err = crush_lang_rust::rust_to_cast("fn main() { let f = |(a, b): (i32, i32)| a + b; println!(\"{}\", f((1, 2))); }")
        .expect_err("must not lower");
    assert!(format!("{err:#}").contains("destructuring"), "{err:#}");
}
