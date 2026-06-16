use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub package: PackageSection,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub dependencies: HashMap<String, Dependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageSection {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default = "default_entry")]
    pub entry: String,
    #[serde(default = "default_edition")]
    pub edition: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Dependency {
    Simple(String),
    Full(DependencyFull),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyFull {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<String>,
}

impl Dependency {
    pub fn resolve_path(&self, manifest_dir: &Path) -> Option<PathBuf> {
        match self {
            Dependency::Simple(p) => Some(manifest_dir.join(p)),
            Dependency::Full(f) => f.path.as_ref().map(|p| manifest_dir.join(p)),
        }
    }
}

fn default_version() -> String {
    "0.1.0".to_string()
}

fn default_entry() -> String {
    "src/main.crush".to_string()
}

fn default_edition() -> String {
    "2024".to_string()
}

impl Manifest {
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_str(&content)
    }

    pub fn from_str(content: &str) -> anyhow::Result<Self> {
        let manifest: Manifest = toml::from_str(content)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.package.name.is_empty() {
            anyhow::bail!("package name cannot be empty");
        }
        if self.package.entry.is_empty() {
            anyhow::bail!("package entry cannot be empty");
        }
        Ok(())
    }

    pub fn to_toml_string(&self) -> anyhow::Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    /// Write serialized manifest to `dir/crush.toml`.
    pub fn write_to_dir(&self, dir: &Path) -> anyhow::Result<PathBuf> {
        let path = dir.join("crush.toml");
        std::fs::write(&path, self.to_toml_string()?)?;
        Ok(path)
    }
}

/// Scaffold a new Crush package directory.
pub fn scaffold_package(dir: &Path, name: &str) -> anyhow::Result<Manifest> {
    let src_dir = dir.join("src");
    std::fs::create_dir_all(&src_dir)?;

    let main_path = src_dir.join("main.crush");
    if !main_path.exists() {
        std::fs::write(
            &main_path,
            "fn main() {\n    io.print(\"hello from Crush\")\n}\n".to_string(),
        )?;
    }

    let manifest = Manifest {
        package: PackageSection {
            name: name.to_string(),
            version: default_version(),
            entry: "src/main.crush".to_string(),
            edition: default_edition(),
            description: Some(format!("{} — a Crush program", name)),
            authors: None,
        },
        dependencies: HashMap::new(),
    };

    manifest.write_to_dir(dir)?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_manifest() {
        let toml = r#"[package]
name = "hello"
version = "0.1.0"
entry = "src/main.crush"
"#;
        let m = Manifest::from_str(toml).unwrap();
        assert_eq!(m.package.name, "hello");
        assert_eq!(m.package.entry, "src/main.crush");
        assert!(m.dependencies.is_empty());
    }

    #[test]
    fn test_manifest_with_deps() {
        let toml = r#"[package]
name = "my-app"
version = "0.1.0"

[dependencies]
stdlib = { path = "../stdlib" }
utils = "0.2.0"
"#;
        let m = Manifest::from_str(toml).unwrap();
        assert_eq!(m.package.name, "my-app");
        assert_eq!(m.dependencies.len(), 2);
        assert!(matches!(&m.dependencies["stdlib"], Dependency::Full(d) if d.path.is_some()));
        assert!(matches!(&m.dependencies["utils"], Dependency::Simple(_)));
    }

    #[test]
    fn test_round_trip() {
        let m = Manifest {
            package: PackageSection {
                name: "test".into(),
                version: "1.0.0".into(),
                entry: "main.crush".into(),
                edition: "2024".into(),
                description: None,
                authors: None,
            },
            dependencies: HashMap::new(),
        };
        let toml = m.to_toml_string().unwrap();
        let m2 = Manifest::from_str(&toml).unwrap();
        assert_eq!(m.package.name, m2.package.name);
        assert_eq!(m.package.version, m2.package.version);
    }

    #[test]
    fn test_scaffold() {
        let dir = tempfile::tempdir().unwrap();
        let m = scaffold_package(dir.path(), "my-pkg").unwrap();
        assert_eq!(m.package.name, "my-pkg");
        assert!(dir.path().join("crush.toml").exists());
        assert!(dir.path().join("src/main.crush").exists());
    }
}
