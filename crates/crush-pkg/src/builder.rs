use std::path::{Path, PathBuf};

use crate::manifest::Manifest;

pub struct DepResolution {
    pub name: String,
    pub root: PathBuf,
    pub source_file: PathBuf,
    pub source: String,
    pub program: Option<crush_vm::Program>,
}

pub struct BuildOutput {
    pub program: crush_vm::Program,
    pub source_files: Vec<PathBuf>,
    pub functions: Vec<String>,
    pub deps: Vec<DepResolution>,
}

pub struct PackageBuilder {
    manifest: Manifest,
    root_dir: PathBuf,
}

impl PackageBuilder {
    pub fn new(manifest: Manifest, root_dir: PathBuf) -> Self {
        Self { manifest, root_dir }
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn check(&self) -> anyhow::Result<()> {
        let sources = self.collect_all_sources()?;
        for (path, _) in &sources {
            let source = std::fs::read_to_string(path)?;
            let program = crush_frontend::compile_crush_source(&source)?;
            if program.functions.is_empty() {
                anyhow::bail!("{}: no functions defined", path.display());
            }
            println!("  checked {}", path.display());
        }
        let deps = self.resolve_deps()?;
        for dep in &deps {
            let program = crush_frontend::compile_crush_source(&dep.source)?;
            if program.functions.is_empty() {
                anyhow::bail!("{}: no functions defined", dep.source_file.display());
            }
            println!("  checked dep {}", dep.source_file.display());
        }
        println!("check passed: {} source(s)", sources.len() + deps.len());
        Ok(())
    }

    pub fn build(&self) -> anyhow::Result<BuildOutput> {
        let deps = self.resolve_deps()?;
        let sources = self.collect_all_sources()?;
        let entry = self.root_dir.join(&self.manifest.capsule.entry);

        let has_entry = sources.iter().any(|(p, _)| p == &entry);
        if !has_entry {
            anyhow::bail!(
                "entry file {} not found; check [capsule].entry",
                entry.display()
            );
        }

        let mut combined = String::new();
        combined.push_str("// Crush package: ");
        combined.push_str(&self.manifest.capsule.name);
        combined.push('\n');

        for (path, _) in &sources {
            let src = std::fs::read_to_string(path)?;
            combined.push_str(&format!("// --- source: {} ---\n", path.display()));
            combined.push_str(&src);
            combined.push('\n');
        }

        let dep_names: Vec<String> = deps.iter().map(|d| d.name.clone()).collect();

        for dep in &deps {
            combined.push_str(&format!("// --- dep: {}\n", dep.name));
            combined.push_str(&dep.source);
            combined.push('\n');
        }

        let program = crush_lang_sdk::compile::compile_crush_source(&combined)?;

        let functions: Vec<String> = program.manifest.functions.keys().cloned().collect();
        let all_source_paths: Vec<PathBuf> = sources.into_iter().map(|(p, _)| p).collect();

        println!(
            "  compiled {} -> {} function(s), {} byte(s)",
            entry.display(),
            functions.len(),
            program.code.len(),
        );
        if !deps.is_empty() {
            println!("  deps: {}", dep_names.join(", "));
        }

        Ok(BuildOutput {
            program,
            source_files: all_source_paths,
            functions,
            deps,
        })
    }

    fn resolve_deps(&self) -> anyhow::Result<Vec<DepResolution>> {
        let mut resolved = Vec::new();
        for dep in &self.manifest.dependencies {
            let name = &dep.name;
            let dep_path = if let Some(path) = &dep.path {
                self.root_dir.join(path)
            } else {
                continue;
            };

            let manifest_path = crate::manifest::manifest_path(&dep_path)
                .ok_or_else(|| anyhow::anyhow!(
                    "dependency '{}': no capsule / crush.toml found at {}",
                    name,
                    dep_path.display()
                ))?;

            let dep_manifest = Manifest::from_file(&manifest_path)?;
            let dep_entry = dep_path.join(&dep_manifest.capsule.entry);
            if !dep_entry.exists() {
                anyhow::bail!(
                    "dependency '{}': entry file not found at {}",
                    name,
                    dep_entry.display()
                );
            }

            let source = std::fs::read_to_string(&dep_entry)?;
            println!("  resolved dep {} -> {}", name, dep_entry.display());

            resolved.push(DepResolution {
                name: name.clone(),
                root: dep_path,
                source_file: dep_entry,
                source,
                program: None,
            });
        }
        Ok(resolved)
    }

    fn collect_all_sources(&self) -> anyhow::Result<Vec<(PathBuf, String)>> {
        let entry = self.root_dir.join(&self.manifest.capsule.entry);
        let mut sources = Vec::new();

        if entry.exists() {
            sources.push((entry, "entry".to_string()));
        } else {
            let src_dir = self.root_dir.join("src");
            if src_dir.exists() {
                for entry in walkdir::WalkDir::new(&src_dir)
                    .into_iter()
                    .filter_map(|e| e.ok())
                {
                    let path = entry.path().to_path_buf();
                    if path.extension().map_or(false, |e| e == "crush") {
                        sources.push((path, "source".to_string()));
                    }
                }
            }
            sources.sort_by(|a, b| a.0.cmp(&b.0));
        }

        Ok(sources)
    }

    pub fn write_output(&self, output: &BuildOutput) -> anyhow::Result<()> {
        let target_dir = self.root_dir.join("target");
        std::fs::create_dir_all(&target_dir)?;

        let name = &self.manifest.capsule.name;

        let cvm_path = target_dir.join(format!("{}.cvm", name));
        let blob = output.program.to_blob();
        std::fs::write(&cvm_path, blob)?;
        println!("  wrote {}", cvm_path.display());

        let casm_path = target_dir.join(format!("{}.casm.json", name));
        let dump = serde_json::json!({
            "version": "1.0",
            "name": name,
            "functions": output.functions,
            "code_len": output.program.code.len(),
            "consts": output.program.consts,
            "manifest": {
                "runtime": output.program.manifest.runtime,
                "permissions": output.program.manifest.permissions,
                "name": output.program.manifest.name,
                "entry": output.program.manifest.entry,
            },
            "code_hex": output.program.code.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(""),
        });
        let json_str = serde_json::to_string_pretty(&dump)?;
        std::fs::write(&casm_path, json_str)?;
        println!("  wrote {}", casm_path.display());

        Ok(())
    }

    pub fn run(&self, _args: &[String]) -> anyhow::Result<crush_vm::VmResult> {
        let output = self.build()?;
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&output.program, &quotas, None)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::scaffold_package;

    fn make_manifest(
        name: &str,
        entry: &str,
        deps: Vec<crate::manifest::Dependency>,
    ) -> Manifest {
        Manifest {
            capsule: crate::manifest::CapsuleSection {
                name: name.to_string(),
                version: "0.1.0".to_string(),
                entry: entry.to_string(),
                language: "crush".to_string(),
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
            capabilities: crate::manifest::CapabilitiesSection::default(),
            resources: crate::manifest::ResourcesSection::default(),
            env: std::collections::HashMap::new(),
            service: None,
            dependencies: deps,
            runtime: None,
        }
    }

    #[test]
    fn test_collect_sources() {
        let dir = tempfile::tempdir().unwrap();
        scaffold_package(dir.path(), "test-pkg").unwrap();

        let manifest = Manifest::from_file(&dir.path().join("capsule.toml")).unwrap();
        let builder = PackageBuilder::new(manifest, dir.path().to_path_buf());
        let sources = builder.collect_all_sources().unwrap();

        assert!(sources.len() >= 1);
        assert!(sources.iter().any(|(p, _)| p.ends_with("src/main.crush")));
    }

    #[test]
    fn test_resolve_path_dep() {
        let dir = tempfile::tempdir().unwrap();

        // Create dep package
        let dep_dir = dir.path().join("dep-lib");
        std::fs::create_dir_all(dep_dir.join("src")).unwrap();
        std::fs::write(
            dep_dir.join("src/lib.crush"),
            "fn greet(name) {\n    io.print(\"hello \" + name)\n}\n",
        ).unwrap();
        let dep_manifest = make_manifest("dep-lib", "src/lib.crush", vec![]);
        dep_manifest.write_to_dir(&dep_dir).unwrap();

        // Create main package
        let main_dir = dir.path().join("main-pkg");
        scaffold_package(&main_dir, "main-pkg").unwrap();

        // Add the path dep
        let dep = crate::manifest::Dependency {
            name: "dep-lib".to_string(),
            version: None,
            path: Some(dep_dir.to_string_lossy().to_string()),
        };
        let mut main_manifest = Manifest::from_file(&main_dir.join("capsule.toml")).unwrap();
        main_manifest.dependencies.push(dep);
        main_manifest.write_to_dir(&main_dir).unwrap();

        let builder = PackageBuilder::new(main_manifest, main_dir);
        let deps = builder.resolve_deps().unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "dep-lib");
    }

    #[test]
    fn test_build_with_dep() {
        let dir = tempfile::tempdir().unwrap();

        // Create dep package
        let dep_dir = dir.path().join("dep-lib");
        std::fs::create_dir_all(dep_dir.join("src")).unwrap();
        std::fs::write(
            dep_dir.join("src/lib.crush"),
            "fn greet(name) {\n    io.print(\"hello \" + name)\n}\n",
        ).unwrap();
        let dep_manifest = make_manifest("dep-lib", "src/lib.crush", vec![]);
        dep_manifest.write_to_dir(&dep_dir).unwrap();

        // Create main package
        let main_dir = dir.path().join("main-pkg");
        std::fs::create_dir_all(main_dir.join("src")).unwrap();
        std::fs::write(
            main_dir.join("src/main.crush"),
            "fn main() {\n    greet(\"world\")\n}\n",
        ).unwrap();
        let dep = crate::manifest::Dependency {
            name: "dep-lib".to_string(),
            version: None,
            path: Some(dep_dir.to_string_lossy().to_string()),
        };
        let main_manifest = make_manifest("main-pkg", "src/main.crush", vec![dep]);
        main_manifest.write_to_dir(&main_dir).unwrap();

        let builder = PackageBuilder::new(main_manifest, main_dir);
        let output = builder.build().unwrap();

        assert!(!output.program.code.is_empty());
    }

    #[test]
    fn test_check_valid_package() {
        let dir = tempfile::tempdir().unwrap();
        scaffold_package(dir.path(), "check-test").unwrap();

        let manifest = Manifest::from_file(&dir.path().join("capsule.toml")).unwrap();
        let builder = PackageBuilder::new(manifest, dir.path().to_path_buf());
        builder.check().unwrap();
    }

    #[test]
    fn test_build_valid_package() {
        let dir = tempfile::tempdir().unwrap();
        scaffold_package(dir.path(), "build-test").unwrap();

        let manifest = Manifest::from_file(&dir.path().join("capsule.toml")).unwrap();
        let builder = PackageBuilder::new(manifest, dir.path().to_path_buf());
        let output = builder.build().unwrap();

        assert!(!output.program.code.is_empty());
        assert!(output.functions.contains(&"main".to_string()));
    }

    #[test]
    fn test_write_output() {
        let dir = tempfile::tempdir().unwrap();
        scaffold_package(dir.path(), "write-test").unwrap();

        let manifest = Manifest::from_file(&dir.path().join("capsule.toml")).unwrap();
        let builder = PackageBuilder::new(manifest, dir.path().to_path_buf());
        let output = builder.build().unwrap();
        builder.write_output(&output).unwrap();

        let target = dir.path().join("target");
        assert!(target.join("write-test.cvm").exists());
        assert!(target.join("write-test.casm.json").exists());

        let cvm_data = std::fs::read(target.join("write-test.cvm")).unwrap();
        let loaded = crush_vm::Program::from_blob(&cvm_data).unwrap();
        assert!(!loaded.code.is_empty());
    }

    #[test]
    fn test_run_built_package() {
        let dir = tempfile::tempdir().unwrap();
        scaffold_package(dir.path(), "run-test").unwrap();

        let manifest = Manifest::from_file(&dir.path().join("capsule.toml")).unwrap();
        let builder = PackageBuilder::new(manifest, dir.path().to_path_buf());
        let result = builder.run(&[]).unwrap();

        assert!(result.halted);
    }
}
