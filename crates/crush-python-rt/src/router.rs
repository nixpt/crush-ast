use std::collections::HashMap;

/// Analysis of Python source code for backend routing decisions.
#[derive(Debug, Default)]
pub struct PythonAnalysis {
    pub has_class: bool,
    pub has_decorator: bool,
    pub has_eval_exec: bool,
    pub has_import: bool,
    pub imports: Vec<String>,
    pub has_c_extension_imports: bool,
    pub has_async: bool,
    pub has_generator: bool,
    pub has_meta_programming: bool,
    pub ast_node_count: usize,
}

/// How well a backend can handle given Python code.
#[derive(Debug, Clone, PartialEq)]
pub enum SupportLevel {
    /// Backend can handle everything natively.
    Native,
    /// Backend can handle it.
    Supported,
    /// Backend can handle it with limitations.
    Partial(Vec<String>),
    /// Backend cannot handle it.
    Unsupported(Vec<String>),
}

impl SupportLevel {
    pub fn is_native(&self) -> bool {
        matches!(self, SupportLevel::Native)
    }
    pub fn is_supported(&self) -> bool {
        matches!(self, SupportLevel::Native | SupportLevel::Supported)
    }
}

/// Which backend to use for executing Python code.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PythonBackend {
    CastTranspile,
    RustPythonEmbedded,
    Subprocess,
}

/// Backend that can execute Python code.
pub trait PythonBackendRunner {
    fn supports(&self, analysis: &PythonAnalysis) -> SupportLevel;
}

/// Simple rule-based analyzer.
pub fn analyze_python(source: &str) -> PythonAnalysis {
    let mut analysis = PythonAnalysis::default();

    for line in source.lines() {
        let line = line.trim();

        if line.starts_with("class ") {
            analysis.has_class = true;
        }
        if line.starts_with("@") && !line.starts_with("@python") && !line.starts_with("@exo") {
            analysis.has_decorator = true;
        }
        if line.contains("eval(") || line.contains("exec(") {
            analysis.has_eval_exec = true;
        }
        if line.starts_with("import ") {
            analysis.has_import = true;
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                analysis.imports.push(parts[1].to_string());
            }
        }
        if line.starts_with("from ") && line.contains(" import ") {
            analysis.has_import = true;
            let mod_part = line.strip_prefix("from ").and_then(|s| s.split(" import ").next());
            if let Some(mod_name) = mod_part {
                analysis.imports.push(mod_name.trim().to_string());
            }
        }
        if line.contains("async ") || line.contains("await ") {
            analysis.has_async = true;
        }
        if line.contains("yield ") {
            analysis.has_generator = true;
        }
        if line.contains("__import__") || line.contains("globals()") || line.contains("locals()") {
            analysis.has_meta_programming = true;
        }
        analysis.ast_node_count += 1;
    }

    // Detect C-extension imports
    let c_ext_modules = ["numpy", "pandas", "torch", "tensorflow", "scipy", "cv2", "PIL", "lxml"];
    for import in &analysis.imports {
        let base = import.split('.').next().unwrap_or("");
        if c_ext_modules.contains(&base) {
            analysis.has_c_extension_imports = true;
        }
    }

    analysis
}

/// Router that selects the best backend for Python code.
pub struct PythonRouter {
    pub transpile_support: Vec<Box<dyn PythonBackendRunner>>,
    pub rustpython_support: Vec<Box<dyn PythonBackendRunner>>,
}

impl PythonRouter {
    pub fn new() -> Self {
        Self {
            transpile_support: vec![Box::new(CastTranspileAnalyzer)],
            rustpython_support: vec![Box::new(RustPythonAnalyzer)],
        }
    }

    pub fn choose_backend(&self, source: &str) -> PythonBackend {
        let analysis = analyze_python(source);

        // Check CAST transpile lane first
        if self
            .transpile_support
            .iter()
            .all(|b| b.supports(&analysis).is_native())
        {
            return PythonBackend::CastTranspile;
        }

        // Check RustPython lane
        if self
            .rustpython_support
            .iter()
            .all(|b| b.supports(&analysis).is_supported())
        {
            return PythonBackend::RustPythonEmbedded;
        }

        // Fallback to subprocess
        PythonBackend::Subprocess
    }
}

struct CastTranspileAnalyzer;

impl PythonBackendRunner for CastTranspileAnalyzer {
    fn supports(&self, analysis: &PythonAnalysis) -> SupportLevel {
        if analysis.has_class
            || analysis.has_decorator
            || analysis.has_eval_exec
            || analysis.has_meta_programming
            || analysis.has_generator
            || analysis.has_async
            || analysis.has_c_extension_imports
        {
            return SupportLevel::Unsupported(vec![
                "dynamic features not supported by CAST transpiler".to_string()
            ]);
        }
        SupportLevel::Native
    }
}

struct RustPythonAnalyzer;

impl PythonBackendRunner for RustPythonAnalyzer {
    fn supports(&self, analysis: &PythonAnalysis) -> SupportLevel {
        if analysis.has_eval_exec || analysis.has_meta_programming {
            return SupportLevel::Unsupported(vec![
                "eval/exec not supported in RustPython lane".to_string()
            ]);
        }
        if analysis.has_c_extension_imports {
            return SupportLevel::Unsupported(vec![
                "C extensions not available in RustPython".to_string()
            ]);
        }
        SupportLevel::Supported
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_arithmetic_routes_to_cast() {
        let source = "x = 1\ny = x + 2\nprint(y)";
        let router = PythonRouter::new();
        assert_eq!(router.choose_backend(source), PythonBackend::CastTranspile);
    }

    #[test]
    fn test_class_routes_to_rustpython() {
        let source = "class User:\n    def __init__(self, name):\n        self.name = name";
        let router = PythonRouter::new();
        assert_eq!(router.choose_backend(source), PythonBackend::RustPythonEmbedded);
    }

    #[test]
    fn test_numpy_routes_to_subprocess() {
        let source = "import numpy as np\nnp.array([1, 2, 3])";
        let router = PythonRouter::new();
        assert_eq!(router.choose_backend(source), PythonBackend::Subprocess);
    }

    #[test]
    fn test_eval_routes_to_subprocess() {
        let source = "eval('1 + 2')";
        let router = PythonRouter::new();
        assert_eq!(router.choose_backend(source), PythonBackend::Subprocess);
    }

    #[test]
    fn test_analyzer_detects_class() {
        let analysis = analyze_python("class Foo:\n    pass");
        assert!(analysis.has_class);
    }

    #[test]
    fn test_analyzer_detects_imports() {
        let analysis = analyze_python("import os\nfrom json import loads");
        assert!(analysis.has_import);
        assert_eq!(analysis.imports.len(), 2);
        assert!(analysis.imports.contains(&"os".to_string()));
    }
}
