use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crush_cast::*;

static WALKER_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_walker_id() -> String {
    let n = WALKER_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("w{}", n)
}

pub trait LanguageWalker {
    fn language(&self) -> &'static str;
    fn extensions(&self) -> &'static [&'static str];

    fn parse(
        &self,
        source: &str,
        filename: Option<&str>,
    ) -> Result<Box<dyn std::any::Any>, WalkerError>;

    fn walk(&self, ast: Box<dyn std::any::Any>) -> Result<Program, WalkerError>;

    fn capabilities(&self) -> LanguageCapabilities {
        LanguageCapabilities::default()
    }
}

#[derive(Debug, Clone)]
pub struct LanguageCapabilities {
    pub version: String,
    pub execution_model: ExecutionModel,
    #[allow(dead_code)]
    pub type_system: TypeSystemFeatures,
    pub stdlib_available: bool,
    pub package_manager: Option<String>,
    pub jit_supported: bool,
    pub native_supported: bool,
}

impl Default for LanguageCapabilities {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            execution_model: ExecutionModel::Interpreted,
            type_system: TypeSystemFeatures::default(),
            stdlib_available: false,
            package_manager: None,
            jit_supported: false,
            native_supported: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ExecutionModel {
    Interpreted,
    Compiled,
    JIT,
    Mixed,
}

#[derive(Debug, Clone)]
pub struct TypeSystemFeatures {
    pub static_typing: bool,
    pub type_inference: bool,
    pub generics: bool,
    pub traits_interfaces: bool,
    pub structural_typing: bool,
}

impl Default for TypeSystemFeatures {
    fn default() -> Self {
        Self {
            static_typing: false,
            type_inference: true,
            generics: false,
            traits_interfaces: false,
            structural_typing: false,
        }
    }
}

pub struct WalkerRegistry {
    walkers: HashMap<String, Box<dyn LanguageWalker>>,
}

pub struct SubprocessWalker {
    language: &'static str,
    extensions: &'static [&'static str],
    binary_name: String,
    capabilities: LanguageCapabilities,
}

impl SubprocessWalker {
    pub fn new(
        language: &'static str,
        extensions: &'static [&'static str],
        binary_name: &str,
        capabilities: LanguageCapabilities,
    ) -> Self {
        Self {
            language,
            extensions,
            binary_name: binary_name.to_string(),
            capabilities,
        }
    }
}

impl LanguageWalker for SubprocessWalker {
    fn language(&self) -> &'static str {
        self.language
    }

    fn extensions(&self) -> &'static [&'static str] {
        self.extensions
    }

    fn parse(
        &self,
        source: &str,
        _filename: Option<&str>,
    ) -> Result<Box<dyn std::any::Any>, WalkerError> {
        Ok(Box::new(source.to_string()))
    }

    fn walk(&self, ast: Box<dyn std::any::Any>) -> Result<Program, WalkerError> {
        let source = ast
            .downcast_ref::<String>()
            .ok_or_else(|| WalkerError::ParseError("Invalid AST type".to_string()))?;

        let temp_filename = format!("walker_{}.tmp", next_walker_id());
        let temp_path = std::env::temp_dir().join(&temp_filename);

        std::fs::write(&temp_path, source).map_err(WalkerError::IoError)?;

        let output = std::process::Command::new(&self.binary_name)
            .arg(&temp_path)
            .output();

        let _ = std::fs::remove_file(&temp_path);

        let output = output.map_err(WalkerError::IoError)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WalkerError::ParseError(format!(
                "Walker binary failed: {}",
                stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let program: Program = serde_json::from_str(&stdout)
            .map_err(|e| WalkerError::CastGenerationError(e.to_string()))?;

        Ok(program)
    }

    fn capabilities(&self) -> LanguageCapabilities {
        self.capabilities.clone()
    }
}

impl WalkerRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            walkers: HashMap::new(),
        };

        registry.register_walker(Box::new(SubprocessWalker::new(
            "python",
            &["py", "pyw"],
            "python_walker",
            LanguageCapabilities {
                version: "3.11".to_string(),
                execution_model: ExecutionModel::Interpreted,
                type_system: TypeSystemFeatures {
                    static_typing: false,
                    type_inference: true,
                    generics: false,
                    traits_interfaces: false,
                    structural_typing: true,
                },
                stdlib_available: true,
                package_manager: Some("pip".to_string()),
                jit_supported: false,
                native_supported: false,
            },
        )));

        registry.register_walker(Box::new(SubprocessWalker::new(
            "javascript",
            &["js", "mjs"],
            "js_walker",
            LanguageCapabilities {
                version: "ES2023".to_string(),
                execution_model: ExecutionModel::JIT,
                type_system: TypeSystemFeatures {
                    static_typing: false,
                    type_inference: true,
                    generics: false,
                    traits_interfaces: false,
                    structural_typing: true,
                },
                stdlib_available: true,
                package_manager: Some("npm".to_string()),
                jit_supported: true,
                native_supported: false,
            },
        )));

        registry.register_walker(Box::new(SubprocessWalker::new(
            "typescript",
            &["ts", "tsx"],
            "js_walker",
            LanguageCapabilities {
                version: "5.0".to_string(),
                execution_model: ExecutionModel::JIT,
                type_system: TypeSystemFeatures {
                    static_typing: true,
                    type_inference: true,
                    generics: true,
                    traits_interfaces: true,
                    structural_typing: true,
                },
                stdlib_available: true,
                package_manager: Some("npm".to_string()),
                jit_supported: true,
                native_supported: false,
            },
        )));

        registry.register_walker(Box::new(SubprocessWalker::new(
            "rust",
            &["rs"],
            "rust_walker",
            LanguageCapabilities {
                version: "1.70".to_string(),
                execution_model: ExecutionModel::Compiled,
                type_system: TypeSystemFeatures {
                    static_typing: true,
                    type_inference: true,
                    generics: true,
                    traits_interfaces: true,
                    structural_typing: false,
                },
                stdlib_available: true,
                package_manager: Some("cargo".to_string()),
                jit_supported: false,
                native_supported: true,
            },
        )));

        registry.register_walker(Box::new(SubprocessWalker::new(
            "go",
            &["go"],
            "go_walker",
            LanguageCapabilities {
                version: "1.21".to_string(),
                execution_model: ExecutionModel::Compiled,
                type_system: TypeSystemFeatures {
                    static_typing: true,
                    type_inference: true,
                    generics: true,
                    traits_interfaces: true,
                    structural_typing: false,
                },
                stdlib_available: true,
                package_manager: Some("go modules".to_string()),
                jit_supported: false,
                native_supported: true,
            },
        )));

        registry.register_walker(Box::new(SubprocessWalker::new(
            "cpp",
            &["cpp", "cc", "cxx", "c++", "hpp", "h"],
            "c_walker",
            LanguageCapabilities {
                version: "C++23".to_string(),
                execution_model: ExecutionModel::Compiled,
                type_system: TypeSystemFeatures {
                    static_typing: true,
                    type_inference: false,
                    generics: true,
                    traits_interfaces: true,
                    structural_typing: false,
                },
                stdlib_available: true,
                package_manager: Some("cmake".to_string()),
                jit_supported: false,
                native_supported: true,
            },
        )));

        registry.register_walker(Box::new(SubprocessWalker::new(
            "bash",
            &["sh", "bash"],
            "bash_walker",
            LanguageCapabilities {
                version: "5.0".to_string(),
                execution_model: ExecutionModel::Interpreted,
                type_system: TypeSystemFeatures::default(),
                stdlib_available: true,
                package_manager: None,
                jit_supported: false,
                native_supported: false,
            },
        )));

        registry.register_walker(Box::new(SubprocessWalker::new(
            "wasm",
            &["wasm"],
            "wasm_walker",
            LanguageCapabilities {
                version: "2.0".to_string(),
                execution_model: ExecutionModel::Compiled,
                type_system: TypeSystemFeatures {
                    static_typing: true,
                    type_inference: false,
                    generics: false,
                    traits_interfaces: false,
                    structural_typing: false,
                },
                stdlib_available: true,
                package_manager: Some("wapm".to_string()),
                jit_supported: true,
                native_supported: true,
            },
        )));

        registry
    }

    pub fn register_walker(&mut self, walker: Box<dyn LanguageWalker>) {
        self.walkers.insert(walker.language().to_string(), walker);
    }

    pub fn get_walker(&self, language: &str) -> Option<&dyn LanguageWalker> {
        self.walkers.get(language).map(|w| &**w)
    }

    pub fn get_walker_for_extension(&self, extension: &str) -> Option<&dyn LanguageWalker> {
        for walker in self.walkers.values() {
            if walker.extensions().contains(&extension) {
                return Some(&**walker);
            }
        }
        None
    }

    pub fn supported_languages(&self) -> Vec<String> {
        self.walkers.keys().cloned().collect()
    }

    pub fn walk_to_cast(
        &self,
        source: &str,
        language: &str,
        filename: Option<&str>,
    ) -> Result<Program, WalkerError> {
        let walker = self
            .get_walker(language)
            .ok_or_else(|| WalkerError::UnsupportedLanguage(language.to_string()))?;

        let ast = walker.parse(source, filename)?;
        walker.walk(ast)
    }

    pub fn auto_walk_to_cast(&self, source: &str, filename: &str) -> Result<Program, WalkerError> {
        let extension = std::path::Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| WalkerError::InvalidFilename(filename.to_string()))?;

        let walker = self
            .get_walker_for_extension(extension)
            .ok_or_else(|| WalkerError::UnsupportedExtension(extension.to_string()))?;

        let ast = walker.parse(source, Some(filename))?;
        walker.walk(ast)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WalkerError {
    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),
    #[error("Unsupported file extension: {0}")]
    UnsupportedExtension(String),
    #[error("Invalid filename: {0}")]
    InvalidFilename(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Semantic error: {0}")]
    SemanticError(String),
    #[error("CAST generation error: {0}")]
    CastGenerationError(String),
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_walker_registry() {
        let registry = WalkerRegistry::new();
        let languages = registry.supported_languages();
        assert!(languages.contains(&"python".to_string()));
        assert!(languages.contains(&"javascript".to_string()));
        assert!(languages.contains(&"rust".to_string()));

        let py_walker = registry.get_walker_for_extension("py");
        assert!(py_walker.is_some());
        assert_eq!(py_walker.unwrap().language(), "python");

        let ts_walker = registry.get_walker_for_extension("ts");
        assert!(ts_walker.is_some());
        assert_eq!(ts_walker.unwrap().language(), "typescript");
    }

    #[test]
    fn test_language_capabilities() {
        let py_walker = SubprocessWalker::new(
            "python",
            &["py", "pyw"],
            "python_walker",
            LanguageCapabilities {
                version: "3.11".to_string(),
                execution_model: ExecutionModel::Interpreted,
                type_system: TypeSystemFeatures {
                    static_typing: false,
                    type_inference: true,
                    generics: false,
                    traits_interfaces: false,
                    structural_typing: true,
                },
                stdlib_available: true,
                package_manager: Some("pip".to_string()),
                jit_supported: false,
                native_supported: false,
            },
        );
        let caps = py_walker.capabilities();
        assert_eq!(caps.version, "3.11");
        assert!(!caps.type_system.static_typing);
        assert!(caps.type_system.type_inference);
        assert_eq!(caps.package_manager, Some("pip".to_string()));
    }
}
