//! Polyglot Import Handler
//!
//! Manages imports within polyglot language blocks (@python, @js, etc.).
//! Provides secure import resolution and sandboxing for multi-language execution.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::import_system::{ImportError, ImportResolution, SecurityContext, TrustLevel};
use crush_cast::ImportStatement;

static SANDBOX_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_sandbox_id() -> String {
    format!("sandbox_{}", SANDBOX_COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// Polyglot execution context
#[derive(Debug)]
pub struct PolyglotContext {
    /// Language-specific import resolvers
    language_resolvers: HashMap<String, LanguageImportResolver>,

    /// Global import cache to avoid duplicate resolutions
    import_cache: HashMap<String, ImportResolution>,

    /// Active sandboxes for each language
    sandboxes: HashMap<String, Sandbox>,
}

/// Language-specific import resolver
#[derive(Debug)]
pub struct LanguageImportResolver {
    /// Language name (python, javascript, rust, etc.)
    language: String,

    /// Language-specific import rules
    import_rules: ImportRules,

    /// Available packages/modules for this language
    #[allow(dead_code)]
    available_modules: HashMap<String, ModuleInfo>,
}

/// Import rules for a specific language
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportRules {
    /// Import syntax patterns
    import_syntax: Vec<String>,

    /// Module path separators
    path_separator: String,

    /// Standard library modules (always available)
    stdlib_modules: Vec<String>,

    /// Restricted modules (security)
    restricted_modules: Vec<String>,

    /// Package managers for this language
    package_managers: Vec<String>,
}

/// Module information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInfo {
    pub name: String,
    pub version: Option<String>,
    pub capabilities: Vec<String>,
    pub security_level: SecurityLevel,
}

/// Security levels for modules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecurityLevel {
    System,
    Trusted,
    Sandboxed,
    Restricted,
    Blocked,
}

/// Sandbox for language execution
#[derive(Debug)]
pub struct Sandbox {
    pub id: String,
    pub language: String,
    pub memory_limit: usize,
    pub cpu_limit: f64,
    pub network_access: bool,
    pub filesystem_access: FileSystemAccess,
    pub active_imports: HashMap<String, ImportResolution>,
}

/// Filesystem access levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileSystemAccess {
    None,
    ReadOnly { paths: Vec<String> },
    ReadWrite { paths: Vec<String> },
    Full,
}

/// Import within polyglot block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolyglotImport {
    /// The import statement
    pub statement: ImportStatement,

    /// Language context
    pub language: String,

    /// Whether this import affects the global scope
    pub global_scope: bool,
}

impl Default for PolyglotContext {
    fn default() -> Self {
        Self::new()
    }
}

impl PolyglotContext {
    pub fn new() -> Self {
        let mut context = Self {
            language_resolvers: HashMap::new(),
            import_cache: HashMap::new(),
            sandboxes: HashMap::new(),
        };

        context.initialize_language_resolvers();
        context
    }

    fn initialize_language_resolvers(&mut self) {
        let python_resolver = LanguageImportResolver {
            language: "python".to_string(),
            import_rules: ImportRules {
                import_syntax: vec![
                    "import {module}".to_string(),
                    "from {module} import {items}".to_string(),
                    "import {module} as {alias}".to_string(),
                ],
                path_separator: ".".to_string(),
                stdlib_modules: vec![
                    "sys".to_string(), "os".to_string(), "json".to_string(),
                    "datetime".to_string(), "math".to_string(), "random".to_string(),
                    "collections".to_string(),
                ],
                restricted_modules: vec![
                    "subprocess".to_string(), "socket".to_string(),
                    "http".to_string(), "urllib".to_string(), "ftplib".to_string(),
                ],
                package_managers: vec!["pip".to_string(), "conda".to_string()],
            },
            available_modules: HashMap::new(),
        };
        self.language_resolvers.insert("python".to_string(), python_resolver);

        let js_resolver = LanguageImportResolver {
            language: "javascript".to_string(),
            import_rules: ImportRules {
                import_syntax: vec![
                    "const {items} = require('{module}')".to_string(),
                    "import {items} from '{module}'".to_string(),
                    "import * as {alias} from '{module}'".to_string(),
                ],
                path_separator: "/".to_string(),
                stdlib_modules: vec![
                    "fs".to_string(), "path".to_string(), "crypto".to_string(),
                    "events".to_string(), "stream".to_string(), "util".to_string(),
                ],
                restricted_modules: vec![
                    "child_process".to_string(), "http".to_string(),
                    "https".to_string(), "net".to_string(), "dgram".to_string(),
                ],
                package_managers: vec!["npm".to_string(), "yarn".to_string()],
            },
            available_modules: HashMap::new(),
        };
        self.language_resolvers.insert("javascript".to_string(), js_resolver);

        let rust_resolver = LanguageImportResolver {
            language: "rust".to_string(),
            import_rules: ImportRules {
                import_syntax: vec![
                    "use {module}::{items}".to_string(),
                    "extern crate {module}".to_string(),
                ],
                path_separator: "::".to_string(),
                stdlib_modules: vec![
                    "std".to_string(), "core".to_string(), "alloc".to_string(),
                    "collections".to_string(), "io".to_string(), "fs".to_string(),
                ],
                restricted_modules: vec![
                    "std::process".to_string(), "std::net".to_string(), "std::fs".to_string(),
                ],
                package_managers: vec!["cargo".to_string()],
            },
            available_modules: HashMap::new(),
        };
        self.language_resolvers.insert("rust".to_string(), rust_resolver);
    }

    /// Resolve imports within a polyglot block
    pub fn resolve_polyglot_imports(
        &mut self,
        language: &str,
        imports: &[ImportStatement],
        code: &str,
    ) -> Result<PolyglotResolution, PolyglotError> {
        let _resolver = self
            .language_resolvers
            .get_mut(language)
            .ok_or_else(|| PolyglotError::UnsupportedLanguage(language.to_string()))?;

        let sandbox_id = self.ensure_sandbox(language);

        let mut resolutions = Vec::new();
        let mut transformed_code = code.to_string();

        for import in imports {
            let resolution = self.resolve_import(import, language)?;
            resolutions.push(resolution.clone());

            transformed_code =
                self.transform_import_code(transformed_code, &resolution, language)?;
        }

        let implicit = self.scan_for_implicit_imports(code, language)?;
        for imp in implicit {
            if !imports
                .iter()
                .any(|existing| self.imports_match(&imp, existing))
            {
                let resolution = self.resolve_import(&imp, language)?;
                resolutions.push(resolution.clone());
            }
        }

        Ok(PolyglotResolution {
            sandbox_id,
            resolutions,
            transformed_code,
            security_context: self.get_sandbox_security(language),
        })
    }

    fn ensure_sandbox(&mut self, language: &str) -> String {
        if !self.sandboxes.contains_key(language) {
            let sandbox = Sandbox {
                id: next_sandbox_id(),
                language: language.to_string(),
                memory_limit: 50 * 1024 * 1024,
                cpu_limit: 0.5,
                network_access: false,
                filesystem_access: FileSystemAccess::ReadOnly {
                    paths: vec![".".to_string()],
                },
                active_imports: HashMap::new(),
            };
            self.sandboxes.insert(language.to_string(), sandbox);
        }

        self.sandboxes[language].id.clone()
    }

    fn resolve_import(
        &mut self,
        import: &ImportStatement,
        language: &str,
    ) -> Result<ImportResolution, PolyglotError> {
        let cache_key = format!("{}:{:?}", language, import);
        if let Some(cached) = self.import_cache.get(&cache_key) {
            return Ok(cached.clone());
        }

        let resolver = self
            .language_resolvers
            .get_mut(language)
            .ok_or_else(|| PolyglotError::UnsupportedLanguage(language.to_string()))?;

        let resolution = resolver.resolve_import(import)?;

        self.import_cache.insert(cache_key, resolution.clone());

        Ok(resolution)
    }

    fn transform_import_code(
        &self,
        code: String,
        resolution: &ImportResolution,
        language: &str,
    ) -> Result<String, PolyglotError> {
        let mut transformed = code;

        match language {
            "python" => {
                for (alias, item) in &resolution.resolved_items {
                    if let crate::import_system::ResolvedItem::PolyglotItem { name, .. } = item {
                        let import_pattern = format!("import {}", name);
                        let sandbox_import = format!("# Sandbox import: {}", name);
                        transformed = transformed.replace(&import_pattern, &sandbox_import);
                        let _ = alias;
                    }
                }
            }
            "javascript" => {
                for (alias, item) in &resolution.resolved_items {
                    if let crate::import_system::ResolvedItem::PolyglotItem { name, .. } = item {
                        let require_pattern = format!("require('{}')", name);
                        let sandbox_require = format!("// Sandbox require: {}", name);
                        transformed = transformed.replace(&require_pattern, &sandbox_require);
                        let _ = alias;
                    }
                }
            }
            _ => {
                for (alias, item) in &resolution.resolved_items {
                    let _ = alias;
                    let _ = item;
                    transformed =
                        format!("// Sandbox import\n{}", transformed);
                }
            }
        }

        Ok(transformed)
    }

    fn scan_for_implicit_imports(
        &self,
        code: &str,
        language: &str,
    ) -> Result<Vec<ImportStatement>, PolyglotError> {
        let mut implicit = Vec::new();

        match language {
            "python" => {
                for line in code.lines() {
                    let line = line.trim();
                    if line.starts_with("import ") {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 2 {
                            implicit.push(ImportStatement::PolyglotModule {
                                language: language.to_string(),
                                module_path: parts[1].to_string(),
                                alias: None,
                                selective: vec![],
                            });
                        }
                    } else if line.starts_with("from ") && line.contains(" import ") {
                        let from_part = line.split(" import ").next().unwrap_or("");
                        let module = from_part.strip_prefix("from ").unwrap_or("").trim();
                        implicit.push(ImportStatement::PolyglotModule {
                            language: language.to_string(),
                            module_path: module.to_string(),
                            alias: None,
                            selective: vec!["*".to_string()],
                        });
                    }
                }
            }
            "javascript" => {
                for line in code.lines() {
                    let line = line.trim();
                    if line.contains("require(") {
                        if let Some(start) = line.find("require('") {
                            if let Some(end) = line[start + 9..].find("')") {
                                let module = &line[start + 9..start + 9 + end];
                                implicit.push(ImportStatement::PolyglotModule {
                                    language: language.to_string(),
                                    module_path: module.to_string(),
                                    alias: None,
                                    selective: vec![],
                                });
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        Ok(implicit)
    }

    fn imports_match(&self, a: &ImportStatement, b: &ImportStatement) -> bool {
        match (a, b) {
            (
                ImportStatement::PolyglotModule {
                    module_path: path_a,
                    language: lang_a,
                    ..
                },
                ImportStatement::PolyglotModule {
                    module_path: path_b,
                    language: lang_b,
                    ..
                },
            ) => path_a == path_b && lang_a == lang_b,
            _ => false,
        }
    }

    fn get_sandbox_security(&self, language: &str) -> SecurityContext {
        SecurityContext {
            allowed_domains: vec![],
            required_permissions: vec![format!("lang.{}", language)],
            trust_level: TrustLevel::Sandboxed,
            sandbox_restrictions: vec![
                format!("language:{}", language),
                "network:disabled".to_string(),
                "filesystem:readonly".to_string(),
            ],
        }
    }
}

impl LanguageImportResolver {
    fn resolve_import(
        &self,
        import: &ImportStatement,
    ) -> Result<ImportResolution, PolyglotError> {
        match import {
            ImportStatement::PolyglotModule {
                module_path, alias, ..
            } => {
                if self.import_rules.stdlib_modules.contains(module_path) {
                    let resolved_item = crate::import_system::ResolvedItem::PolyglotItem {
                        language: self.language.clone(),
                        name: module_path.clone(),
                        signature: format!("{}::{}", self.language, module_path),
                        sandbox_id: "stdlib".to_string(),
                    };

                    let alias_name = alias.as_ref().unwrap_or(module_path);
                    let resolved_items = HashMap::from([(alias_name.clone(), resolved_item)]);

                    return Ok(ImportResolution {
                        original_import: import.clone(),
                        resolved_items,
                        dependencies: vec![],
                        security_context: SecurityContext {
                            allowed_domains: vec![],
                            required_permissions: vec![],
                            trust_level: TrustLevel::System,
                            sandbox_restrictions: vec![],
                        },
                    });
                }

                if self.import_rules.restricted_modules.contains(module_path) {
                    return Err(PolyglotError::RestrictedModule(module_path.clone()));
                }

                let resolved_item = crate::import_system::ResolvedItem::PolyglotItem {
                    language: self.language.clone(),
                    name: module_path.clone(),
                    signature: format!("{}::{}", self.language, module_path),
                    sandbox_id: next_sandbox_id(),
                };

                let alias_name = alias.as_ref().unwrap_or(module_path);
                let resolved_items = HashMap::from([(alias_name.clone(), resolved_item)]);

                Ok(ImportResolution {
                    original_import: import.clone(),
                    resolved_items,
                    dependencies: vec![],
                    security_context: SecurityContext {
                        allowed_domains: vec![],
                        required_permissions: vec![format!("lang.{}.import", self.language)],
                        trust_level: TrustLevel::Sandboxed,
                        sandbox_restrictions: vec![
                            format!("language:{}", self.language),
                            format!("module:{}", module_path),
                        ],
                    },
                })
            }
            _ => Err(PolyglotError::UnsupportedImportType),
        }
    }
}

/// Polyglot resolution result
#[derive(Debug)]
pub struct PolyglotResolution {
    pub sandbox_id: String,
    pub resolutions: Vec<ImportResolution>,
    pub transformed_code: String,
    pub security_context: SecurityContext,
}

/// Polyglot execution errors
#[derive(Debug, thiserror::Error)]
pub enum PolyglotError {
    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),

    #[error("Sandbox not found: {0}")]
    SandboxNotFound(String),

    #[error("Restricted module: {0}")]
    RestrictedModule(String),

    #[error("Unsupported import type")]
    UnsupportedImportType,

    #[error("Import resolution failed: {0}")]
    ImportResolutionFailed(#[from] ImportError),

    #[error("Code transformation failed: {0}")]
    CodeTransformationFailed(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
}
