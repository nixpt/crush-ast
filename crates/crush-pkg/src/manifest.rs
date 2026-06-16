use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Locate manifest file
// ---------------------------------------------------------------------------

pub fn manifest_path(dir: &Path) -> Option<PathBuf> {
    for name in ["capsule.toml", "Capsule.toml", "crush.toml", "Crush.toml"] {
        let p = dir.join(name);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Canonical TOML schema
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub capsule: CapsuleSection,
    #[serde(default)]
    pub capabilities: CapabilitiesSection,
    #[serde(default)]
    pub resources: ResourcesSection,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<ServiceConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<Dependency>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<RuntimeSection>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapsuleSection {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default = "default_entry")]
    pub entry: String,
    #[serde(default)]
    pub language: String,
    // Policy / daemon fields
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_access: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seccomp_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rootless: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_limit_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_limit_percent: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pids_limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readonly_root: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_paths: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denied_paths: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_point_sha256: Option<String>,
    // Legacy aliases (not serialized back out)
    #[serde(default, skip_serializing)]
    pub entry_point: Option<String>,
    #[serde(default, skip_serializing)]
    pub capsule_type: Option<String>,
    #[serde(default, skip_serializing)]
    pub id: Option<String>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub capsule_kind: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilitiesSection {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub optional: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourcesSection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threads: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gas: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeSection {
    #[serde(default)]
    pub autostart: bool,
    #[serde(default)]
    pub priority: String,
    #[serde(default)]
    pub restart_policy: RestartPolicy,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServiceConfig {
    #[serde(default)]
    pub r#type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RestartPolicy {
    No,
    Always,
    OnFailure,
    OnFailureWithBackoff,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self::No
    }
}

// ---------------------------------------------------------------------------
// Dependency types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

fn default_version() -> String {
    "0.1.0".to_string()
}

fn default_entry() -> String {
    "src/main.crush".to_string()
}

// ---------------------------------------------------------------------------
// Capsule type / runtime enums
// ---------------------------------------------------------------------------

/// Capsule type — determines which runner to use
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CapsuleType {
    #[default]
    Auto,
    Crush,
    Native,
    Container,
    Script(ScriptRuntime),
}

/// Script runtime preference
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ScriptRuntime {
    #[default]
    Bun,
    Node,
    Deno,
    Python,
}

/// Auto-detected payload format based on file extension / magic bytes
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PayloadFormat {
    Casm,
    JavaScript,
    TypeScript,
    Python,
    NativeElf,
    NativeMachO,
    NativePe,
    Container,
    Unknown,
}

impl PayloadFormat {
    pub fn from_path(path: &std::path::Path) -> Self {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            match ext.to_lowercase().as_str() {
                "casm" | "casmb" => PayloadFormat::Casm,
                "js" | "mjs" | "cjs" => PayloadFormat::JavaScript,
                "ts" | "mts" | "cts" | "tsx" => PayloadFormat::TypeScript,
                "py" => PayloadFormat::Python,
                _ => PayloadFormat::Unknown,
            }
        } else {
            PayloadFormat::Unknown
        }
    }

    pub fn from_magic(bytes: &[u8]) -> Self {
        if bytes.len() < 4 {
            return PayloadFormat::Unknown;
        }
        if bytes.starts_with(&[0x7F, 0x45, 0x4C, 0x46]) {
            return PayloadFormat::NativeElf;
        }
        if bytes.starts_with(&[0xCF, 0xFA, 0xED, 0xFE])
            || bytes.starts_with(&[0xFE, 0xED, 0xFA, 0xCF])
        {
            return PayloadFormat::NativeMachO;
        }
        if bytes.starts_with(&[0x4D, 0x5A]) {
            return PayloadFormat::NativePe;
        }
        if bytes.starts_with(&[0x7B]) {
            return PayloadFormat::Casm;
        }
        PayloadFormat::Unknown
    }

    pub fn script_runtime(&self) -> Option<ScriptRuntime> {
        match self {
            PayloadFormat::JavaScript | PayloadFormat::TypeScript => Some(ScriptRuntime::Bun),
            PayloadFormat::Python => Some(ScriptRuntime::Python),
            _ => None,
        }
    }
}

/// Map language string from manifest to CapsuleType
pub fn language_to_capsule_type(language: &str) -> CapsuleType {
    match language {
        "crush" => CapsuleType::Crush,
        "native" | "rust" | "c" => CapsuleType::Native,
        "container" => CapsuleType::Container,
        "javascript" | "js" | "ts" | "typescript" | "bun" => CapsuleType::Script(ScriptRuntime::Bun),
        "node" | "nodejs" => CapsuleType::Script(ScriptRuntime::Node),
        "deno" => CapsuleType::Script(ScriptRuntime::Deno),
        "python" | "py" => CapsuleType::Script(ScriptRuntime::Python),
        _ => CapsuleType::Auto,
    }
}

// ---------------------------------------------------------------------------
// Language helpers
// ---------------------------------------------------------------------------

fn detect_language_from_entry(entry: &str) -> String {
    let ext = Path::new(entry)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "crush" => "crush",
        "ts" | "js" | "mts" | "cts" => "javascript",
        "py" => "python",
        "rs" => "rust",
        "c" | "cpp" | "cc" => "native",
        _ => "crush",
    }
    .to_string()
}

// ---------------------------------------------------------------------------
// Parse / serialize
// ---------------------------------------------------------------------------

impl Manifest {
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_str(&content, path)
    }

    pub fn from_str(content: &str, _source_path: &Path) -> anyhow::Result<Self> {
        let mut table: toml::Table = toml::from_str(content)?;

        // Hard errors on shapes that cannot be unambiguously migrated
        if table.contains_key("permissions") {
            anyhow::bail!(
                "Capsule.toml uses legacy [permissions] section, which cannot be auto-migrated. \
                 Use:\n\n  [capsule]\n  name = \"...\"\n  entry = \"...\"\n\n  [capabilities]\n  required = [...]"
            );
        }
        if table.contains_key("entrypoints") {
            anyhow::bail!(
                "Capsule.toml uses legacy [entrypoints] section, which cannot be auto-migrated. \
                 Remove [entrypoints] and set `entry = \"...\"` under [capsule]."
            );
        }

        // Auto-migrate [package] → [capsule]
        if let Some(package) = table.remove("package") {
            if let Some(name) = package.get("name").cloned() {
                let capsule = table
                    .entry("capsule")
                    .or_insert_with(|| toml::Value::Table(toml::Table::new()))
                    .as_table_mut()
                    .ok_or_else(|| anyhow::anyhow!("[capsule] must be a table"))?;
                if !capsule.contains_key("name") {
                    capsule.insert("name".to_string(), name);
                }
            }
        }

        if !table.contains_key("capsule") {
            anyhow::bail!("Missing [capsule] section in manifest");
        }

        // Auto-migrate entry_point → entry
        {
            let capsule = table["capsule"]
                .as_table_mut()
                .ok_or_else(|| anyhow::anyhow!("[capsule] must be a table"))?;
            if let Some(ep) = capsule.remove("entry_point") {
                if !capsule.contains_key("entry") {
                    capsule.insert("entry".to_string(), ep);
                }
            }
        }

        // Auto-migrate capsule_type → language
        {
            let capsule = table["capsule"]
                .as_table_mut()
                .ok_or_else(|| anyhow::anyhow!("[capsule] must be a table"))?;
            if let Some(ct) = capsule.remove("capsule_type") {
                if !capsule.contains_key("language") {
                    capsule.insert("language".to_string(), ct);
                }
            }
        }

        // Auto-migrate id → name
        {
            let capsule = table["capsule"]
                .as_table_mut()
                .ok_or_else(|| anyhow::anyhow!("[capsule] must be a table"))?;
            if let Some(id) = capsule.remove("id") {
                if !capsule.contains_key("name") {
                    capsule.insert("name".to_string(), id);
                }
            }
        }

        let value = toml::Value::Table(table);
        let mut manifest: Manifest = value.try_into()
            .map_err(|e| anyhow::anyhow!("Failed to parse manifest: {}", e))?;

        // Auto-detect language if not set
        if manifest.capsule.language.is_empty() {
            manifest.capsule.language = detect_language_from_entry(&manifest.capsule.entry);
        }

        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.capsule.name.is_empty() {
            anyhow::bail!("capsule name cannot be empty");
        }
        if self.capsule.entry.is_empty() {
            anyhow::bail!("capsule entry cannot be empty");
        }
        Ok(())
    }

    pub fn to_toml_string(&self) -> anyhow::Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    pub fn write_to_dir(&self, dir: &Path) -> anyhow::Result<PathBuf> {
        let path = dir.join("capsule.toml");
        std::fs::write(&path, self.to_toml_string()?)?;
        Ok(path)
    }
}

// ---------------------------------------------------------------------------
// Scaffold
// ---------------------------------------------------------------------------

pub fn scaffold_package(dir: &Path, name: &str) -> anyhow::Result<Manifest> {
    let src_dir = dir.join("src");
    std::fs::create_dir_all(&src_dir)?;

    let main_path = src_dir.join("main.crush");
    if !main_path.exists() {
        std::fs::write(&main_path, "fn main() {\n    io.print(\"hello from Crush\")\n}\n")?;
    }

    let manifest = Manifest {
        capsule: CapsuleSection {
            name: name.to_string(),
            version: default_version(),
            description: Some(format!("{} — a Crush program", name)),
            author: None,
            entry: "src/main.crush".to_string(),
            language: "crush".to_string(),
            network_access: None,
            seccomp_profile: None,
            rootless: None,
            memory_limit_mb: None,
            cpu_limit_percent: None,
            pids_limit: None,
            readonly_root: None,
            allowed_paths: None,
            denied_paths: None,
            entry_point_sha256: None,
            entry_point: None,
            capsule_type: None,
            id: None,
            capsule_kind: None,
        },
        capabilities: CapabilitiesSection::default(),
        resources: ResourcesSection::default(),
        env: HashMap::new(),
        service: None,
        dependencies: Vec::new(),
        runtime: None,
    };

    manifest.write_to_dir(dir)?;
    Ok(manifest)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_canonical() {
        let toml = r#"[capsule]
name = "test-capsule"
version = "0.1.0"
entry = "main.crush"

[capabilities]
required = ["fs", "time"]
optional = []

[resources]
memory_mb = 64
threads = 2
"#;
        let m = Manifest::from_str(toml, Path::new("")).unwrap();
        assert_eq!(m.capsule.name, "test-capsule");
        assert_eq!(m.capsule.entry, "main.crush");
        assert_eq!(m.capsule.language, "crush");
        assert_eq!(m.capabilities.required.len(), 2);
        assert_eq!(m.resources.memory_mb, Some(64));
        assert_eq!(m.resources.threads, Some(2));
    }

    #[test]
    fn auto_migrate_package_section() {
        let toml = r#"[package]
name = "legacy-pkg"
version = "0.1.0"

[capabilities]
required = ["io"]
"#;
        let m = Manifest::from_str(toml, Path::new("")).unwrap();
        assert_eq!(m.capsule.name, "legacy-pkg");
    }

    #[test]
    fn auto_migrate_entry_point() {
        let toml = r#"[capsule]
name = "legacy-ep"
entry_point = "old.ts"
"#;
        let m = Manifest::from_str(toml, Path::new("")).unwrap();
        assert_eq!(m.capsule.entry, "old.ts");
        assert_eq!(m.capsule.language, "javascript");
    }

    #[test]
    fn permissions_is_hard_error() {
        let toml = r#"[capsule]
name = "bad"

[permissions]
network = true
"#;
        let err = Manifest::from_str(toml, Path::new("")).unwrap_err().to_string();
        assert!(err.contains("legacy [permissions]"));
    }

    #[test]
    fn round_trip() {
        let m = Manifest {
            capsule: CapsuleSection {
                name: "test".into(),
                version: "1.0.0".into(),
                entry: "main.crush".into(),
                language: "crush".into(),
                description: None,
                author: None,
                network_access: None,
                seccomp_profile: None,
                rootless: None,
                memory_limit_mb: None,
                cpu_limit_percent: None,
                pids_limit: None,
                readonly_root: None,
                allowed_paths: None,
                denied_paths: None,
                entry_point_sha256: None,
                entry_point: None,
                capsule_type: None,
                id: None,
                capsule_kind: None,
            },
            capabilities: CapabilitiesSection::default(),
            resources: ResourcesSection::default(),
            env: HashMap::new(),
            service: None,
            dependencies: Vec::new(),
            runtime: None,
        };
        let toml = m.to_toml_string().unwrap();
        let m2 = Manifest::from_str(&toml, Path::new("")).unwrap();
        assert_eq!(m.capsule.name, m2.capsule.name);
        assert_eq!(m.capsule.version, m2.capsule.version);
    }

    #[test]
    fn scaffold() {
        let dir = tempfile::tempdir().unwrap();
        let m = scaffold_package(dir.path(), "my-pkg").unwrap();
        assert_eq!(m.capsule.name, "my-pkg");
        assert!(dir.path().join("capsule.toml").exists());
        assert!(dir.path().join("src/main.crush").exists());
    }

    #[test]
    fn language_auto_detect() {
        let toml = r#"[capsule]
name = "auto"
entry = "app.py"
"#;
        let m = Manifest::from_str(toml, Path::new("")).unwrap();
        assert_eq!(m.capsule.language, "python");
    }

    #[test]
    fn env_section() {
        let toml = r#"[capsule]
name = "env-test"
entry = "main.crush"

[env]
LOG_LEVEL = "debug"
API_BASE = "http://localhost:8080"
"#;
        let m = Manifest::from_str(toml, Path::new("")).unwrap();
        assert_eq!(m.env.get("LOG_LEVEL").map(String::as_str), Some("debug"));
        assert_eq!(m.env.len(), 2);
    }

    #[test]
    fn dependencies() {
        let toml = r#"[capsule]
name = "my-app"
entry = "main.crush"

[[dependencies]]
name = "stdlib"
path = "../stdlib"
"#;
        let m = Manifest::from_str(toml, Path::new("")).unwrap();
        assert_eq!(m.dependencies.len(), 1);
        assert_eq!(m.dependencies[0].name, "stdlib");
    }
}
