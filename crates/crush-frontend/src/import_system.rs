use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crush_cast::{ExternalResourceType, ImportStatement};

static HANDLE_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_handle() -> String {
    let n = HANDLE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("h{}", n)
}

#[derive(Debug, Clone)]
pub struct ImportResolution {
    pub original_import: ImportStatement,
    pub resolved_items: HashMap<String, ResolvedItem>,
    pub dependencies: Vec<String>,
    pub security_context: SecurityContext,
}

#[derive(Debug, Clone)]
pub enum ResolvedItem {
    CrushItem {
        name: String,
        item_type: CrushItemType,
        definition: serde_json::Value,
    },
    PolyglotItem {
        language: String,
        name: String,
        signature: String,
        sandbox_id: String,
    },
    MCPTool {
        server_id: String,
        tool_name: String,
        schema: serde_json::Value,
    },
    Capability {
        path: String,
        permissions: Vec<String>,
        handle: String,
    },
    SecureEnvItem {
        key: String,
        value: Option<String>,
        handle: String,
        db_path: Option<String>,
    },
    SecureEnvModule {
        db_path: Option<String>,
        available_keys: Vec<String>,
        handle: String,
    },
}

#[derive(Debug, Clone)]
pub enum CrushItemType {
    Function,
    Type,
    Module,
    Constant,
    Capability,
}

#[derive(Debug, Clone)]
pub struct SecurityContext {
    pub allowed_domains: Vec<String>,
    pub required_permissions: Vec<String>,
    pub trust_level: TrustLevel,
    pub sandbox_restrictions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TrustLevel {
    System,
    Trusted,
    Sandboxed,
    Isolated,
}

pub struct ImportResolver {
    registry: HashMap<String, ModuleDefinition>,
    #[allow(dead_code)]
    active_imports: HashMap<String, ImportResolution>,
    security_policy: SecurityPolicy,
}

#[derive(Debug, Clone)]
pub struct ModuleDefinition {
    pub name: String,
    pub version: String,
    pub exports: HashMap<String, CrushItemType>,
    pub dependencies: Vec<String>,
    pub security_requirements: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SecurityPolicy {
    pub allow_network_imports: bool,
    pub allow_file_imports: bool,
    pub allow_git_imports: bool,
    pub trusted_domains: Vec<String>,
    pub max_import_depth: usize,
}

impl Default for ImportResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ImportResolver {
    pub fn new() -> Self {
        Self {
            registry: Self::load_builtin_modules(),
            active_imports: HashMap::new(),
            security_policy: SecurityPolicy {
                allow_network_imports: true,
                allow_file_imports: true,
                allow_git_imports: true,
                trusted_domains: vec![
                    "github.com".to_string(),
                    "crates.io".to_string(),
                    "npmjs.com".to_string(),
                ],
                max_import_depth: 10,
            },
        }
    }

    pub fn resolve_import(
        &mut self,
        import: ImportStatement,
    ) -> Result<ImportResolution, ImportError> {
        match &import {
            ImportStatement::CrushModule {
                module_path,
                alias,
                selective,
            } => self.resolve_crush_module(module_path, alias.as_deref(), selective),
            ImportStatement::PolyglotModule {
                language,
                module_path,
                alias,
                selective,
            } => self.resolve_polyglot_module(language, module_path, alias.as_deref(), selective),
            ImportStatement::MCPImport {
                server_url,
                tools,
                alias,
            } => self.resolve_mcp_import(server_url, tools, alias.as_deref()),
            ImportStatement::Capability {
                capability_path,
                permissions,
                alias,
            } => self.resolve_capability(capability_path, permissions, alias.as_deref()),
            ImportStatement::External {
                uri,
                resource_type,
                alias,
            } => self.resolve_external_import(uri, resource_type, alias.as_deref()),
            ImportStatement::SecureEnv {
                keys,
                alias,
                db_path,
            } => self.resolve_secure_env(keys, alias.as_deref(), db_path.as_deref()),
        }
    }

    fn resolve_crush_module(
        &self,
        module_path: &str,
        alias: Option<&str>,
        selective: &[String],
    ) -> Result<ImportResolution, ImportError> {
        let module_def = self
            .registry
            .get(module_path)
            .ok_or_else(|| ImportError::ModuleNotFound(module_path.to_string()))?;

        let mut resolved_items = HashMap::new();
        let items_to_import = if selective.is_empty() {
            module_def.exports.keys().cloned().collect::<Vec<_>>()
        } else {
            selective.to_vec()
        };

        for item_name in items_to_import {
            if let Some(item_type) = module_def.exports.get(&item_name) {
                let resolved_item = ResolvedItem::CrushItem {
                    name: item_name.clone(),
                    item_type: item_type.clone(),
                    definition: serde_json::json!({
                        "module": module_path,
                        "type": format!("{:?}", item_type),
                        "version": module_def.version
                    }),
                };
                let alias_name = alias.unwrap_or(&item_name);
                resolved_items.insert(alias_name.to_string(), resolved_item);
            }
        }

        Ok(ImportResolution {
            original_import: ImportStatement::CrushModule {
                module_path: module_path.to_string(),
                alias: alias.map(|s| s.to_string()),
                selective: selective.to_vec(),
            },
            resolved_items,
            dependencies: module_def.dependencies.clone(),
            security_context: SecurityContext {
                allowed_domains: vec![],
                required_permissions: module_def.security_requirements.clone(),
                trust_level: TrustLevel::Trusted,
                sandbox_restrictions: vec![],
            },
        })
    }

    fn resolve_polyglot_module(
        &self,
        language: &str,
        module_path: &str,
        alias: Option<&str>,
        selective: &[String],
    ) -> Result<ImportResolution, ImportError> {
        let sandbox_id = format!("polyglot_{}_{}", language, next_handle());
        let mut resolved_items = HashMap::new();
        let items_to_import = if selective.is_empty() {
            vec!["*".to_string()]
        } else {
            selective.to_vec()
        };

        for item_name in items_to_import {
            let resolved_item = ResolvedItem::PolyglotItem {
                language: language.to_string(),
                name: item_name.clone(),
                signature: format!("{}::{}", module_path, item_name),
                sandbox_id: sandbox_id.clone(),
            };
            let alias_name = alias.unwrap_or(&item_name);
            resolved_items.insert(alias_name.to_string(), resolved_item);
        }

        Ok(ImportResolution {
            original_import: ImportStatement::PolyglotModule {
                language: language.to_string(),
                module_path: module_path.to_string(),
                alias: alias.map(|s| s.to_string()),
                selective: selective.to_vec(),
            },
            resolved_items,
            dependencies: vec![],
            security_context: SecurityContext {
                allowed_domains: vec![],
                required_permissions: vec![format!("lang.{}", language)],
                trust_level: TrustLevel::Sandboxed,
                sandbox_restrictions: vec![
                    format!("language:{}", language),
                    "network:disabled".to_string(),
                    "filesystem:readonly".to_string(),
                ],
            },
        })
    }

    fn resolve_mcp_import(
        &self,
        server_url: &str,
        tools: &[String],
        alias: Option<&str>,
    ) -> Result<ImportResolution, ImportError> {
        if !server_url.starts_with("https://") {
            return Err(ImportError::InvalidUrl(server_url.to_string()));
        }
        let domain = server_url
            .strip_prefix("https://")
            .and_then(|s| s.split('/').next())
            .unwrap_or("");

        if domain.is_empty() {
            return Err(ImportError::InvalidUrl(server_url.to_string()));
        }

        if !self
            .security_policy
            .trusted_domains
            .iter()
            .any(|trusted| domain == trusted || domain.ends_with(&format!(".{}", trusted)))
        {
            return Err(ImportError::UntrustedDomain(domain.to_string()));
        }

        let mut resolved_items = HashMap::new();
        let server_id = format!("mcp_{}", next_handle());

        for tool_name in tools {
            let resolved_item = ResolvedItem::MCPTool {
                server_id: server_id.clone(),
                tool_name: tool_name.clone(),
                schema: serde_json::json!({
                    "type": "function",
                    "server": server_url,
                    "tool": tool_name
                }),
            };
            let alias_name = alias
                .map(|a| format!("{}.{}", a, tool_name))
                .unwrap_or_else(|| tool_name.clone());
            resolved_items.insert(alias_name, resolved_item);
        }

        Ok(ImportResolution {
            original_import: ImportStatement::MCPImport {
                server_url: server_url.to_string(),
                tools: tools.to_vec(),
                alias: alias.map(|s| s.to_string()),
            },
            resolved_items,
            dependencies: vec![],
            security_context: SecurityContext {
                allowed_domains: vec![domain.to_string()],
                required_permissions: vec!["network.http".to_string(), "mcp.client".to_string()],
                trust_level: TrustLevel::Sandboxed,
                sandbox_restrictions: vec![
                    "network:http_only".to_string(),
                    format!("domain:{}", domain),
                ],
            },
        })
    }

    fn resolve_capability(
        &self,
        capability_path: &str,
        permissions: &[String],
        alias: Option<&str>,
    ) -> Result<ImportResolution, ImportError> {
        if !self.is_valid_capability(capability_path) {
            return Err(ImportError::InvalidCapability(capability_path.to_string()));
        }
        for permission in permissions {
            if !self.is_valid_permission(permission) {
                return Err(ImportError::InvalidPermission(permission.clone()));
            }
        }

        let handle = next_handle();
        let resolved_item = ResolvedItem::Capability {
            path: capability_path.to_string(),
            permissions: permissions.to_vec(),
            handle: handle.clone(),
        };
        let alias_name = alias.unwrap_or(capability_path);
        let resolved_items = HashMap::from([(alias_name.to_string(), resolved_item)]);

        Ok(ImportResolution {
            original_import: ImportStatement::Capability {
                capability_path: capability_path.to_string(),
                permissions: permissions.to_vec(),
                alias: alias.map(|s| s.to_string()),
            },
            resolved_items,
            dependencies: vec![],
            security_context: SecurityContext {
                allowed_domains: vec![],
                required_permissions: permissions.to_vec(),
                trust_level: TrustLevel::System,
                sandbox_restrictions: vec![],
            },
        })
    }

    fn resolve_external_import(
        &self,
        uri: &str,
        resource_type: &ExternalResourceType,
        alias: Option<&str>,
    ) -> Result<ImportResolution, ImportError> {
        match resource_type {
            ExternalResourceType::Http => {
                if !uri.starts_with("https://") {
                    return Err(ImportError::InsecureUri(uri.to_string()));
                }
                let domain = uri
                    .strip_prefix("https://")
                    .and_then(|s| s.split('/').next())
                    .unwrap_or("");
                if domain.is_empty() {
                    return Err(ImportError::InvalidUrl(uri.to_string()));
                }
                if !self
                    .security_policy
                    .trusted_domains
                    .contains(&domain.to_string())
                {
                    return Err(ImportError::UntrustedDomain(domain.to_string()));
                }
            }
            ExternalResourceType::Git => {
                if !uri.starts_with("https://") && !uri.starts_with("git@") {
                    return Err(ImportError::InvalidUri(uri.to_string()));
                }
            }
            ExternalResourceType::File => {
                if !uri.starts_with("file://") {
                    return Err(ImportError::InvalidUri(uri.to_string()));
                }
            }
            ExternalResourceType::Database => {}
            ExternalResourceType::API { format: _ } => {}
        }

        let resolved_item = match resource_type {
            ExternalResourceType::Http => ResolvedItem::CrushItem {
                name: "http_resource".to_string(),
                item_type: CrushItemType::Constant,
                definition: serde_json::json!({
                    "uri": uri,
                    "type": "http",
                    "fetched_at": std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                }),
            },
            ExternalResourceType::Git => ResolvedItem::CrushItem {
                name: "git_repository".to_string(),
                item_type: CrushItemType::Module,
                definition: serde_json::json!({
                    "uri": uri,
                    "type": "git",
                    "cloned": false
                }),
            },
            _ => ResolvedItem::CrushItem {
                name: "external_resource".to_string(),
                item_type: CrushItemType::Constant,
                definition: serde_json::json!({
                    "uri": uri,
                    "resource_type": format!("{:?}", resource_type)
                }),
            },
        };

        let alias_name = alias.unwrap_or("imported_resource");
        let resolved_items = HashMap::from([(alias_name.to_string(), resolved_item)]);

        Ok(ImportResolution {
            original_import: ImportStatement::External {
                uri: uri.to_string(),
                resource_type: resource_type.clone(),
                alias: alias.map(|s| s.to_string()),
            },
            resolved_items,
            dependencies: vec![],
            security_context: SecurityContext {
                allowed_domains: vec![],
                required_permissions: vec![format!("external.{:?}", resource_type)],
                trust_level: TrustLevel::Sandboxed,
                sandbox_restrictions: vec![
                    format!("resource_type:{:?}", resource_type),
                    "network:restricted".to_string(),
                ],
            },
        })
    }

    fn resolve_secure_env(
        &self,
        keys: &[String],
        alias: Option<&str>,
        db_path: Option<&str>,
    ) -> Result<ImportResolution, ImportError> {
        let handle = next_handle();
        let mut resolved_items = HashMap::new();

        if keys.is_empty() {
            let resolved_item = ResolvedItem::SecureEnvModule {
                db_path: db_path.map(|s| s.to_string()),
                available_keys: vec![],
                handle: handle.clone(),
            };
            let alias_name = alias.unwrap_or("secrets");
            resolved_items.insert(alias_name.to_string(), resolved_item);
        } else {
            for key_name in keys {
                let resolved_item = ResolvedItem::SecureEnvItem {
                    key: key_name.clone(),
                    value: None,
                    handle: handle.clone(),
                    db_path: db_path.map(|s| s.to_string()),
                };
                resolved_items.insert(key_name.clone(), resolved_item);
            }
        }

        Ok(ImportResolution {
            original_import: ImportStatement::SecureEnv {
                keys: keys.to_vec(),
                alias: alias.map(|s| s.to_string()),
                db_path: db_path.map(|s| s.to_string()),
            },
            resolved_items,
            dependencies: vec![],
            security_context: SecurityContext {
                allowed_domains: vec![],
                required_permissions: vec!["secrets.read".to_string()],
                trust_level: TrustLevel::System,
                sandbox_restrictions: vec![
                    "secrets:encrypted_only".to_string(),
                    "no_logging".to_string(),
                ],
            },
        })
    }

    fn load_builtin_modules() -> HashMap<String, ModuleDefinition> {
        let mut registry = HashMap::new();
        registry.insert(
            "io".to_string(),
            ModuleDefinition {
                name: "io".to_string(),
                version: "1.0".to_string(),
                exports: HashMap::from([
                    ("print".to_string(), CrushItemType::Function),
                    ("read".to_string(), CrushItemType::Function),
                    ("write".to_string(), CrushItemType::Function),
                ]),
                dependencies: vec![],
                security_requirements: vec!["io.basic".to_string()],
            },
        );
        registry.insert(
            "fs".to_string(),
            ModuleDefinition {
                name: "fs".to_string(),
                version: "1.0".to_string(),
                exports: HashMap::from([
                    ("read_file".to_string(), CrushItemType::Function),
                    ("write_file".to_string(), CrushItemType::Function),
                    ("list_dir".to_string(), CrushItemType::Function),
                ]),
                dependencies: vec![],
                security_requirements: vec!["fs.read".to_string(), "fs.write".to_string()],
            },
        );
        registry.insert(
            "net".to_string(),
            ModuleDefinition {
                name: "net".to_string(),
                version: "1.0".to_string(),
                exports: HashMap::from([
                    ("http_get".to_string(), CrushItemType::Function),
                    ("http_post".to_string(), CrushItemType::Function),
                ]),
                dependencies: vec![],
                security_requirements: vec!["net.http".to_string()],
            },
        );
        registry
    }

    fn is_valid_capability(&self, capability_path: &str) -> bool {
        capability_path.contains('.') && !capability_path.contains("..")
    }

    fn is_valid_permission(&self, permission: &str) -> bool {
        permission.contains('.')
            && permission
                .chars()
                .all(|c| c.is_alphanumeric() || c == '.' || c == '_' || c == '-')
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("Module not found: {0}")]
    ModuleNotFound(String),
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
    #[error("Insecure URI (not HTTPS): {0}")]
    InsecureUri(String),
    #[error("Invalid URI: {0}")]
    InvalidUri(String),
    #[error("Untrusted domain: {0}")]
    UntrustedDomain(String),
    #[error("Invalid capability: {0}")]
    InvalidCapability(String),
    #[error("Invalid permission: {0}")]
    InvalidPermission(String),
    #[error("Import cycle detected")]
    ImportCycle,
    #[error("Network import not allowed")]
    NetworkImportDisabled,
    #[error("Security policy violation: {0}")]
    SecurityViolation(String),
    #[error("Resolution failed: {0}")]
    ResolutionFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crush_module_import() {
        let mut resolver = ImportResolver::new();
        let import = ImportStatement::CrushModule {
            module_path: "io".to_string(),
            alias: None,
            selective: vec![],
        };
        let result = resolver.resolve_import(import);
        assert!(result.is_ok());
        let resolution = result.unwrap();
        assert!(resolution.resolved_items.contains_key("print"));
        assert!(resolution.resolved_items.contains_key("read"));
    }

    #[test]
    fn test_polyglot_module_import() {
        let mut resolver = ImportResolver::new();
        let import = ImportStatement::PolyglotModule {
            language: "python".to_string(),
            module_path: "sys".to_string(),
            alias: Some("system".to_string()),
            selective: vec!["version".to_string()],
        };
        let result = resolver.resolve_import(import);
        assert!(result.is_ok());
        let resolution = result.unwrap();
        assert!(resolution.resolved_items.contains_key("system"));
        assert_eq!(
            resolution.security_context.trust_level,
            TrustLevel::Sandboxed
        );
    }

    #[test]
    fn test_mcp_import() {
        let mut resolver = ImportResolver::new();
        let import = ImportStatement::MCPImport {
            server_url: "https://api.github.com".to_string(),
            tools: vec!["issues.list".to_string()],
            alias: None,
        };
        let result = resolver.resolve_import(import);
        assert!(result.is_ok());
        let resolution = result.unwrap();
        assert!(resolution.resolved_items.contains_key("issues.list"));
    }

    #[test]
    fn test_invalid_domain() {
        let mut resolver = ImportResolver::new();
        let import = ImportStatement::MCPImport {
            server_url: "https://evil.com".to_string(),
            tools: vec!["malicious".to_string()],
            alias: None,
        };
        let result = resolver.resolve_import(import);
        assert!(matches!(result, Err(ImportError::UntrustedDomain(_))));
    }

    #[test]
    fn test_capability_import() {
        let mut resolver = ImportResolver::new();
        let import = ImportStatement::Capability {
            capability_path: "fs.read".to_string(),
            permissions: vec!["fs.read".to_string()],
            alias: Some("file_reader".to_string()),
        };
        let result = resolver.resolve_import(import);
        assert!(result.is_ok());
        let resolution = result.unwrap();
        assert!(resolution.resolved_items.contains_key("file_reader"));
    }

    #[test]
    fn test_secure_env_import_specific_keys() {
        let mut resolver = ImportResolver::new();
        let import = ImportStatement::SecureEnv {
            keys: vec!["DATABASE_URL".to_string(), "API_KEY".to_string()],
            alias: None,
            db_path: None,
        };
        let result = resolver.resolve_import(import);
        assert!(result.is_ok());
        let resolution = result.unwrap();
        assert!(resolution.resolved_items.contains_key("DATABASE_URL"));
        assert!(resolution.resolved_items.contains_key("API_KEY"));
        assert_eq!(resolution.security_context.trust_level, TrustLevel::System);
        assert!(
            resolution
                .security_context
                .required_permissions
                .contains(&"secrets.read".to_string())
        );
        assert!(
            resolution
                .security_context
                .sandbox_restrictions
                .contains(&"no_logging".to_string())
        );
    }

    #[test]
    fn test_secure_env_import_all_with_alias() {
        let mut resolver = ImportResolver::new();
        let import = ImportStatement::SecureEnv {
            keys: vec![],
            alias: Some("env".to_string()),
            db_path: Some("/custom/secrets.db".to_string()),
        };
        let result = resolver.resolve_import(import);
        assert!(result.is_ok());
        let resolution = result.unwrap();
        assert!(resolution.resolved_items.contains_key("env"));
        if let Some(ResolvedItem::SecureEnvModule { db_path, .. }) =
            resolution.resolved_items.get("env")
        {
            assert_eq!(db_path, &Some("/custom/secrets.db".to_string()));
        } else {
            panic!("Expected SecureEnvModule");
        }
    }
}
