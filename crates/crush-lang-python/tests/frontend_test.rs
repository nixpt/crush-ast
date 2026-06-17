use crush_lang_python::PythonFrontend;
use walker_core::{Frontend};

/// Helper: just run analyze (not full pipeline) on source.
fn test_analyze(source: &str) -> walker_core::FeatureReport {
    let frontend = PythonFrontend;
    let ast = frontend.parse(source).unwrap();
    frontend.analyze(&ast).unwrap()
}

#[test]
fn test_python_frontend_detects_dangerous_imports() {
    let report = test_analyze("import os\nx = 1\n");
    assert!(report.dangerous_imports.contains(&"os".to_string()));
    assert!(!report.can_lower_safely());
    assert!(report.uses_imports.contains(&"os".to_string()));
}

#[test]
fn test_python_frontend_safe_code() {
    let source = "x = 42\nprint(x + 1)\n";
    let frontend = PythonFrontend;
    let (report, program) = walker_core::frontend_pipeline(&frontend, source).unwrap();
    assert!(report.dangerous_imports.is_empty());
    assert!(report.can_lower_safely());
    assert!(program.functions.contains_key("main"));
}

#[test]
fn test_python_frontend_detects_classes_and_async() {
    let report = test_analyze("class Foo:\n    pass\n\nasync def bar():\n    pass\n");
    assert!(report.uses_classes);
    assert!(report.uses_async);
}

#[test]
fn test_python_frontend_detects_exceptions() {
    let report = test_analyze("try:\n    raise ValueError('bad')\nexcept:\n    pass\n");
    assert!(report.uses_exceptions);
}

#[test]
fn test_frontend_detects_imports() {
    let report = test_analyze("import math\nfrom json import loads\n");
    assert!(report.uses_imports.contains(&"math".to_string()));
    assert!(report.uses_imports.contains(&"json".to_string()));
}

#[test]
fn test_frontend_meta_programming() {
    let report = test_analyze("global x\nx = 1\n");
    assert!(report.uses_meta_programming);
}
