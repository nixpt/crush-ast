use std::collections::HashSet;
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

// ---------------------------------------------------------------------------
// `capsule.toml` dead-code detector
// ---------------------------------------------------------------------------
//
// Conservative lint rule (thinker-validated design, "Rule A" + "Path 1"):
// scan the raw TOML text in the `[env]` table (the section alone is a
// single-line `KEY = "value"` table per TOML grammar); any key whose VALUE
// matches one of the known debug-copy-paste placeholders is "dead code".
// Restricting to the `[env]` table prevents false positives on legitimate
// `[capsule]` metadata fields (name/version/entry etc.) — a developer
// who types `name = "alpha"` is naming their package, not leaving a
// placeholder. Raw text scanning (rather than reparsing through
// `Manifest::from_str`) preserves the line numbers we need for the
// canonical four-tuple wire shape.
//
// The detector does NOT take a `std::fs::File` — callers thread in
// `content: &str` so tests can hand-craft fixtures without touching disk
// (`tests::lint_capsule_toml_*` below) and production reads the file via
// `std::fs::read_to_string` at the call site. Zero new dependencies;
// matches the project's "no extra crate" idiom for parser-internal lints.

/// Placeholder values that trigger a dead-code finding when they
/// appear as an `[env]`-table value. Conservative — none of
/// these would ever be a legitimate production env value:
/// debug-copy-paste markers the developer forgot to clean up.
pub const DEAD_CODE_PLACEHOLDERS: &[&str] =
    &["alpha", "beta", "TEMP", "UNUSED", "DEBUG", "FIXME", "TODO"];

// Dead-code rules table ---------------------------------------------------
//
// Each row maps a TOML section to one rule family. The single dispatch
// loop in `lint_capsule_toml_with_entry` walks `DEAD_CODE_RULES` and
// emits one `LintFinding` per match — so the number of emit-sites stays
// proportional to the rule-row count, and the strict-mode CI gate
// composes without rule-count drift.

/// Discriminator for the dead-code rule families. Each variant owns the
/// emit-shapes for its findings (message + hint format strings live
/// inline in the dispatch loop, keyed off this enum so new variants are
/// a single match arm).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleKind {
    /// Fires when any `[env]` (or `[env.<sub>]` sub-table) value matches
    /// one of the `DEAD_CODE_PLACEHOLDERS` symbols.
    PlaceholderValue,
    /// Fires when a section contains an obsolete key that has been
    /// renamed by a documented migration (e.g. `capsule_type` →
    /// `language`, see `manifest.rs::capsule_section_auto_migrate`).
    ObsoleteKey,
}

/// One row in the dead-code rules table. New rules become `const` rows
/// — adding a row is the canonical way to extend the detector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeadCodeRule {
    /// Section name without sub-section; matches both `[section]` and
    /// `[section.<sub>]` headers. e.g. `"env"` matches both `[env]` and
    /// `[env.production]`.
    pub section: &'static str,
    /// Key inside the section. Empty (`""`) means "any key in the
    /// section" (used by `PlaceholderValue`). Exact match against the
    /// TOML key for `ObsoleteKey`.
    pub key: &'static str,
    /// Replacement key name — only consulted by `ObsoleteKey`; the
    /// dispatch loop templates the `hint` field from it. Empty for
    /// `PlaceholderValue`.
    pub replacement: &'static str,
    pub kind: RuleKind,
}

/// All rules the lint dispatcher checks. Add new rules here as `const`
/// rows — no other code in `builder.rs` needs to change.
pub const DEAD_CODE_RULES: &[DeadCodeRule] = &[
    DeadCodeRule {
        section: "env",
        key: "",
        replacement: "",
        kind: RuleKind::PlaceholderValue,
    },
    DeadCodeRule {
        section: "capsule",
        key: "capsule_type",
        replacement: "language",
        kind: RuleKind::ObsoleteKey,
    },
];

/// Wire code emitted for capsule-toml dead-code findings.
/// Centralized as `LintFinding::CODE` so the four-tuple lockdown
/// test in `main.rs` and a future re-emit sites share one
/// literal source of truth — re-routing the code means a
/// one-line change here and zero test rewrites.
#[derive(Debug, Clone, PartialEq)]
pub struct LintFinding {
    /// 1-based line number where the finding's source line lives in `capsule.toml`.
    pub line: u32,
    /// TOML key that triggered the finding (env key, `capsule_type`, dep `name`, …).
    pub key: String,
    /// Canonical human-readable description — what the finding is about.
    pub message: String,
    /// Canonical remediation hint — what the user should do to fix it.
    pub hint: String,
}

impl LintFinding {
    pub const CODE: &'static str = "E-BUILDER";
}

/// Scan a `capsule.toml` raw text body for dead-code findings in
/// the `[env]` table. Returns one [`LintFinding`] per matching
/// key. Line numbers are 1-based (TOML convention; what editor
/// consumers expect).
///
/// The shape returned here maps 1:1 to the canonical seven-field
/// wire: `code = LintFinding::CODE`, `level = "note"`,
/// `file = "capsule.toml"`, `line = finding.line`,
/// `message = "dead-code: unused capsule field `<key>`"`,
/// `hint = "remove or rename `<key>` and any reference to it"`.
/// `emit_post_dispatch_lint` in `main.rs` is the call site that
/// wires these findings through `emit_diag` with strict-mode
/// honored.
/// Standalone form (no entry-file cross-reference). The dispatch loop
/// still walks every row of `DEAD_CODE_RULES`, but any rule that
/// depends on the entry file (currently none — the cross-reference
/// pass runs on collected `[dependencies].name` rows regardless) is
/// skipped here. Kept as the canonical entry point so existing tests
/// that don't need entry awareness stay simple.
pub fn lint_capsule_toml(content: &str) -> Vec<LintFinding> {
    lint_capsule_toml_with_entry(content, None)
}

/// Entry-aware form. Pass `Some(entry_refs)` to enable a
/// cross-reference pass that flags `[dependencies].name` rows the
/// entry file does not reference. The single dispatch loop walks
/// `DEAD_CODE_RULES` and emits one `LintFinding` per match — one emit
/// site per rule row, composable under the strict-mode CI gate.
pub fn lint_capsule_toml_with_entry(
    content: &str,
    entry_refs: Option<&HashSet<String>>,
) -> Vec<LintFinding> {
    let mut findings = Vec::new();
    let mut section_root = String::new();
    // (line_no, dep_name) collected during the main scan for the
    // post-loop cross-reference pass.
    let mut dep_name_lines: Vec<(u32, String)> = Vec::new();

    for (idx, raw) in content.lines().enumerate() {
        let line_no = (idx + 1) as u32;
        // Strip inline comments before any further parsing — a line
        // like `capsule_type = "Crush" # old key` would otherwise
        // capture the comment text into the value slot.
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        // Section header — supports `[x]` and `[[x]]`
        // (array-of-tables, TOML).
        if line.starts_with('[') && line.ends_with(']') {
            let raw_header = &line[1..line.len() - 1];
            section_root = if raw_header.starts_with('[') && raw_header.ends_with(']') {
                raw_header[1..raw_header.len() - 1].to_string() // [[x]] form
            } else {
                raw_header.to_string()
            };
            continue;
        }
        let (key, value) = match parse_generic_kv(line) {
            Some(kv) => kv,
            None => continue,
        };
        // Compare the section prefix only — `[env.production]` and
        // `[capsule.entry]` both match rules keyed on `"env"` /
        // `"capsule"` respectively.
        let section_prefix = section_root.split('.').next().unwrap_or(&section_root);

        // Single dispatch loop — every rule family lives in one match arm.
        for rule in DEAD_CODE_RULES {
            // Exact section-name match (deliberately NOT prefix
            // matching) — `[env.production]` stays out of scope per
            // the `lint_capsule_toml_sub_table_clears_env_flag`
            // lockdown that pre-dates this rule-table extension.
            if rule.section != section_root {
                continue;
            }
            match rule.kind {
                RuleKind::PlaceholderValue => {
                    if DEAD_CODE_PLACEHOLDERS.contains(&value) {
                        findings.push(LintFinding {
                            line: line_no,
                            key: key.to_string(),
                            message: format!(
                                "placeholder value `{value}` on [env] key `{key}`"
                            ),
                            hint: format!(
                                "on key `{key}`: replace `{value}` with a real value, or remove `{key}`"
                            ),
                        });
                    }
                }
                // The ObsoleteKey hint also mentions the rule's
                // canonical replacement token (e.g. `language` for
                // `capsule_type`) so the JSON-mode hint slot carries
                // a hard guarantee about the migration path.
                RuleKind::ObsoleteKey => {
                    if rule.key == key {
                        findings.push(LintFinding {
                            line: line_no,
                            key: key.to_string(),
                            message: format!(
                                "obsolete key `{key}` on [{section_root}]"
                            ),
                            hint: format!(
                                "rename `{key}` to `{}` (or remove the field)", rule.replacement
                            ),
                        });
                    }
                }
            }
        }

        // Collect dep rows during the main scan so the
        // cross-reference pass can run after.
        if section_root == "dependencies" && key == "name" && !value.is_empty() {
            dep_name_lines.push((line_no, value.to_string()));
        }
    }

    // Cross-reference pass — only fires when the caller supplied
    // entry_refs. If the entry file is missing/unreadable, the caller
    // passes `None` and this branch is skipped, per the
    // graceful-degradation contract.
    if let Some(refs) = entry_refs {
        for (line, dep) in dep_name_lines {
            if !refs.contains(&dep) {
                findings.push(LintFinding {
                    line,
                    key: dep.clone(),
                    message: format!(
                        "dependency `{dep}` declared in [dependencies] but not referenced by entry file"
                    ),
                    hint: format!(
                        "remove `{dep}` from [dependencies] or reference it in the entry file"
                    ),
                });
            }
        }
    }

    findings
}

/// Generic `key = value` parser used by `lint_capsule_toml_with_entry`.
/// Quotes are stripped (single or double). Empty values return
/// `None`. Caller is expected to have stripped inline comments and
/// surrounding whitespace before calling.
fn parse_generic_kv(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.splitn(2, '=');
    let key = parts.next()?.trim();
    let raw_value = parts.next()?.trim();
    let value = raw_value.trim_matches(|c| c == '"' || c == '\'');
    if value.is_empty() {
        return None;
    }
    Some((key, value))
}

/// Scan an entry file for identifier-like references. Returns
/// `None` on read failure — caller should then skip any
/// entry-aware lint rule, per the graceful-degradation contract.
///
/// Heuristic: split on any char that isn't alphanumeric / `_` / `-`
/// so hyphenated dep names like `my-lib` survive intact as one
/// token. This is intentionally lossy — the lint should be cheap.
pub fn scan_entry_file_references(path: &Path) -> Option<HashSet<String>> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut refs = HashSet::new();
    let mut cur = String::new();
    let mut in_comment = false;
    // Track the previous character so the `#` → comment
    // transition is gated on whitespace-or-BOL. Without this
    // gate, `#` inside a string literal (e.g. a URL fragment
    // like `"docs.md#install"`) would silently flip into
    // comment mode and strip the rest of the line — truncating
    // identifier fragments the entry file legitimately
    // references by URL.
    let mut prev: Option<char> = None;
    for c in content.chars() {
        if c == '\n' {
            in_comment = false;
            if !cur.is_empty() {
                refs.insert(std::mem::take(&mut cur));
            }
            prev = Some('\n');
            continue;
        }
        if in_comment {
            prev = Some(c);
            continue;
        }
        // `#` only flips into comment mode when preceded by
        // whitespace OR a newline (BOL row-wise). Anything else
        // (alphanumeric, `_`, `-`, `"`, etc.) means `c` is in
        // mid-token context — `#` is then a separator like any
        // other non-alphanumeric/`_`/`-` char.
        //
        //   prev == None           → start of file (BOL)
        //   prev == Some('\n')     → BOL after a newline
        //   prev == Some(c.is_whitespace()) → after whitespace
        //   otherwise              → mid-token (separator)
        if c == '#'
            && prev.map_or(true, |p| p == '\n' || p.is_whitespace())
        {
            in_comment = true;
            prev = Some(c);
            continue;
        }
        if c.is_alphanumeric() || c == '_' || c == '-' {
            cur.push(c);
        } else if !cur.is_empty() {
            refs.insert(std::mem::take(&mut cur));
        }
        prev = Some(c);
    }
    if !cur.is_empty() {
        refs.insert(cur);
    }
    Some(refs)
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

            let manifest_path = crate::manifest::manifest_path(&dep_path).ok_or_else(|| {
                anyhow::anyhow!(
                    "dependency '{}': no capsule / crush.toml found at {}",
                    name,
                    dep_path.display()
                )
            })?;

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

    fn make_manifest(name: &str, entry: &str, deps: Vec<crate::manifest::Dependency>) -> Manifest {
        Manifest {
            capsule: crate::manifest::CapsuleSection {
                name: name.to_string(),
                version: "0.1.0".to_string(),
                entry: entry.to_string(),
                language: "crush".to_string(),
                runtime_version: None,
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
        )
        .unwrap();
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
        )
        .unwrap();
        let dep_manifest = make_manifest("dep-lib", "src/lib.crush", vec![]);
        dep_manifest.write_to_dir(&dep_dir).unwrap();

        // Create main package
        let main_dir = dir.path().join("main-pkg");
        std::fs::create_dir_all(main_dir.join("src")).unwrap();
        std::fs::write(
            main_dir.join("src/main.crush"),
            "fn main() {\n    greet(\"world\")\n}\n",
        )
        .unwrap();
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

    // ----------------------------------------------------------------
    // capsule.toml dead-code detector lockdown
    //
    // Coverage matrix:
    //   1. Clean capsule.toml (no placeholders, no findings).
    //   2. alpha placeholder in [env] found with correct line+key.
    //   3. Placeholder outside [env] (e.g. in [capsule].name) is
    //      NOT flagged — rule is scoped to the [env] table only,
    //      preventing false positives on legitimate metadata.
    //   4. Sub-table section header (`[env.production]`) clears
    //      the in_env_section flag — scoped sections only,
    //      sub-tables are out-of-scope until a future iteration
    //      explicitly opens them up.
    // ----------------------------------------------------------------

    #[test]
    fn lint_capsule_toml_returns_empty_for_clean_capsule() {
        let content = "\
[capsule]
name = \"my-pkg\"
version = \"0.1.0\"
entry = \"src/main.crush\"

[env]
LOG_LEVEL = \"debug\"
API_BASE = \"https://api.example.com\"
FEATURE_FLAG_X = \"on\"
";
        let findings = lint_capsule_toml(content);
        assert!(
            findings.is_empty(),
            "clean env section must produce zero findings, got: {:?}",
            findings
        );
    }

    #[test]
    fn lint_capsule_toml_emits_canonical_finding_for_alpha_in_env() {
        let content = "\
[capsule]
name = \"my-pkg\"

[env]
LOG_LEVEL = \"debug\"
TEMP_ENDPOINT = \"alpha\"
API_BASE = \"https://api.example.com\"
";
        let findings = lint_capsule_toml(content);
        assert_eq!(
            findings.len(),
            1,
            "exactly one placeholder finding expected, got: {:?}",
            findings
        );
        let f = &findings[0];
        // 1-based line numbers (TOML/editor convention). Fixture
        // has 7 lines (the leading `"\\\n` continuation strip + 6
        // visible lines plus a blank separator on line 3), so
        // TEMP_ENDPOINT is on line 6.
        assert_eq!(
            f.line, 6,
            "TEMP_ENDPOINT line is 6 (1-based) in the 7-line fixture, got: {}",
            f.line
        );
        assert_eq!(f.key, "TEMP_ENDPOINT");
        // Canonical wire code — same literal the struct form in
        // `main.rs::tests::strict_mode_pins_warning_class_four_tuple_in_diag_record`
        // uses, so a refactor that changes either fails both tests.
        assert_eq!(LintFinding::CODE, "E-BUILDER");
        // Verify the placeholder was the one expected (and not
        // a coincidental match in a comment line; the section
        // flag + `parse_env_kv` together enforce this).
        assert!(DEAD_CODE_PLACEHOLDERS.contains(&"alpha"));
    }

    #[test]
    fn lint_capsule_toml_ignores_placeholder_outside_env_section() {
        // `[capsule].name = "alpha"` is a legitimate package name
        // (a developer could legitimately call their experimental
        // package "alpha"); the rule must NOT flag it. Only `[env]`
        // table values are scanned.
        let content = "\
[capsule]
name = \"alpha\"

[env]
LOG_LEVEL = \"debug\"
";
        let findings = lint_capsule_toml(content);
        assert!(
            findings.is_empty(),
            "placeholder in [capsule].name must NOT be flagged, got: {:?}",
            findings
        );
    }

    #[test]
    fn lint_capsule_toml_sub_table_clears_env_flag() {
        // Opening `[env.production]` (a TOML dotted sub-table)
        // must clear the in-env flag so values in the sub-table
        // are not flagged at the [env] section's scope.
        let content = "\
[env]
LEGIT_VALUE = \"on\"

[env.production]
PROD_FLAG = \"alpha\"
";
        let findings = lint_capsule_toml(content);
        assert!(
            findings.is_empty(),
            "[env.production] sub-table must be out-of-scope, got: {:?}",
            findings
        );
    }

    #[test]
    fn lint_capsule_toml_flags_comments_and_blank_lines_safely() {
        // Comments (`# ...`) and blank lines inside [env] must be
        // parsed without panic and without false findings.
        let content = "\
[env]
# this is a placeholder: alpha
LOG_LEVEL = \"debug\"

ANOTHER_REAL = \"value\"
";
        let findings = lint_capsule_toml(content);
        assert!(
            findings.is_empty(),
            "comments must not be parsed as KEY = value, got: {:?}",
            findings
        );
    }

    // ─── rule-table extension lockdown tests ──────────────────
    //
    // Pins every row in `DEAD_CODE_RULES` with at least one
    // positive + one negative case so future contributors can't
    // silently disable a rule row.

    /// Obsolete-key rule (`capsule_type → language`): positive.
    /// Finding fires with the replacement name baked into the hint.
    #[test]
    fn lint_capsule_toml_obsolete_capsule_type_emits_replacement_hint() {
        let content = "[capsule]\nname = \"x\"\ncapsule_type = \"Crush\"\n";
        let findings = lint_capsule_toml(content);
        assert_eq!(findings.len(), 1, "exactly one finding");
        assert_eq!(findings[0].key, "capsule_type");
        assert!(
            findings[0].message.contains("obsolete key `capsule_type`"),
            "message should declare the obsolete key, got: {:?}",
            findings[0].message
        );
        assert!(
            findings[0].hint.contains("`language`"),
            "hint should name the replacement, got: {:?}",
            findings[0].hint
        );
    }

    /// Obsolete-key rule: negative. Manifest already uses
    /// `language` → no finding. Pins that the rule does not
    /// fire on the canonical post-migration shape.
    #[test]
    fn lint_capsule_toml_obsolete_capsule_type_absent_emits_no_finding() {
        let content = "[capsule]\nname = \"x\"\nlanguage = \"crush\"\n";
        assert!(
            lint_capsule_toml(content).is_empty(),
            "manifest using `language` (the replacement) must not flag"
        );
    }

    /// Trap 2 pin — inline `# comment` on the same line as a kv
    /// pair must NOT pollute the captured value. Without
    /// `.split('#')` in the dispatcher the captured value would
    /// be `"Crush" # old key` rather than `"Crush"`.
    #[test]
    fn lint_capsule_toml_inline_comment_does_not_pollute_value() {
        let content =
            "[capsule]\nname = \"x\"\ncapsule_type = \"Crush\" # old key\n";
        let findings = lint_capsule_toml(content);
        assert_eq!(findings.len(), 1, "exactly one finding");
        // The comment would have garbled the rule message; if we
        // see a substring of ` # old key` in `message`, the
        // dispatcher is leaking the comment into the value slot.
        assert!(
            !findings[0].message.contains("old key"),
            "inline comment leaked into value, got: {:?}",
            findings[0].message
        );
        assert!(
            findings[0].message.contains("obsolete key"),
            "rule semantic preserved, got: {:?}",
            findings[0].message
        );
    }

    /// Trap 1 pin — `[env.<sub>]` sub-tables stay out-of-scope
    /// (per deliberate OLD behavior, pinned by the existing
    /// `lint_capsule_toml_sub_table_clears_env_flag` lockdown).
    #[test]
    fn lint_capsule_toml_dotted_env_subtable_out_of_scope() {
        let content = "[env.production]\nLOG_LEVEL = \"TODO\"\n";
        let findings = lint_capsule_toml(content);
        assert!(
            findings.is_empty(),
            "dotted env sub-table must stay out-of-scope, got: {:?}",
            findings
        );
    }

    /// Cross-reference pass: dep declared but absent from refs →
    /// finding fires with key + dep-specific message.
    #[test]
    fn lint_capsule_toml_unreferenced_dep_flagged_when_entry_refs_supplied() {
        let content = "\
            [[dependencies]]\nname = \"alpha-dep\"\n\
            \n\
            [[dependencies]]\nname = \"beta-dep\"\n";
        let mut refs = HashSet::new();
        refs.insert("alpha-dep".to_string());
        let findings = lint_capsule_toml_with_entry(content, Some(&refs));
        assert_eq!(findings.len(), 1, "only beta-dep is unreferenced");
        assert_eq!(findings[0].key, "beta-dep");
        assert!(
            findings[0].message.contains("not referenced by entry file"),
            "message should describe the rule, got: {:?}",
            findings[0].message
        );
    }

    /// Cross-reference pass: every dep listed in refs →
    /// zero findings (the pass-thru round-trip).
    #[test]
    fn lint_capsule_toml_referenced_dep_passes_when_entry_refs_supplied() {
        let content = "\
            [[dependencies]]\nname = \"alpha-dep\"\n\
            \n\
            [[dependencies]]\nname = \"beta-dep\"\n";
        let mut refs = HashSet::new();
        refs.insert("alpha-dep".to_string());
        refs.insert("beta-dep".to_string());
        let findings = lint_capsule_toml_with_entry(content, Some(&refs));
        assert!(
            findings.is_empty(),
            "all-deps-referenced must → zero findings, got: {:?}",
            findings
        );
    }

    /// Graceful degradation: `entry_refs = None` must NOT fire
    /// the dep cross-reference rule, even if the manifest has
    /// `[dependencies]` rows. Without this, callers that can't
    /// read the entry file would unexpectedly flag every dep.
    #[test]
    fn lint_capsule_toml_no_entry_refs_skips_dep_cross_reference_pass() {
        let content = "[[dependencies]]\nname = \"alpha-dep\"\n";
        let findings = lint_capsule_toml_with_entry(content, None);
        assert!(
            findings.is_empty(),
            "entry_refs=None must skip dep cross-reference, got: {:?}",
            findings
        );
    }

    // ----------------------------------------------------------------
    // `scan_entry_file_references` whitespace-or-BOL `#` gate lockdown
    // ----------------------------------------------------------------
    // Round-2 reviewer-flagged limitation: the previous scanner
    // stripped `# ...` comments unconditionally on any `#` char,
    // which would silently truncate identifier fragments inside
    // string literals (URL fragments like `"docs.md#install"`). The
    // fix gates the `in_comment = true` flip on
    // `prev_char.is_whitespace() || prev_char == '\n' ||
    //  prev is None (start of file)`. The four tests below verify
    // (a) URL fragments survive, (b) true line-comment markers
    // (preceded by whitespace) still strip, (c) `key#suffix`
    // identifiers split rather than comment, and (d) `#`-at-file-
    // start triggers comment mode (the `prev == None` arm).

    /// Trap-3 pin: `#` inside a string literal (URL fragment)
    /// MUST be treated as a separator, NOT a comment marker.
    /// The `prev` character at the `#` is `"` (the closing
    /// string-literal quote), which is neither whitespace nor
    /// newline, so the comment flip is suppressed. The
    /// following token (`install`) survives intact.
    #[test]
    fn scan_entry_file_references_url_fragment_in_string_not_a_comment() {
        let dir = tempfile::tempdir().expect("tempdir creation");
        let path = dir.path().join("main.crush");
        std::fs::write(
            &path,
            "import \"docs.md#install\"\nimport alpha-dep\n",
        )
        .expect("write entry file");
        let refs =
            scan_entry_file_references(&path).expect("scanner must read the on-disk file");
        // Tokens that survive the URL-string scan — `#` is a
        // separator inside the string, not a comment. Both
        // surrounding imports must also register their deps.
        assert!(
            refs.contains("docs"),
            "URL host token `docs` must survive the `#`-gated scan (refs: {:?})",
            refs
        );
        assert!(
            refs.contains("md"),
            "URL stem token `md` must survive (refs: {:?})",
            refs
        );
        assert!(
            refs.contains("install"),
            "URL fragment token `install` must SURVIVE — the bug being \
             fixed is precisely the truncation of this identifier \
             (refs: {:?})",
            refs
        );
        assert!(refs.contains("alpha-dep"));
        assert!(refs.contains("import"));
    }

    /// Regression pin: a `key # comment` line (whitespace preceding
    /// `#`) MUST still flip into comment mode — the fix above must
    /// not over-correct and silently allow real comments through.
    /// Plus a BOL-after-newline case (covers `prev == Some('\n')`).
    #[test]
    fn scan_entry_file_references_whitespace_then_hash_strips_comment() {
        let dir = tempfile::tempdir().expect("tempdir creation");
        let path = dir.path().join("main.crush");
        std::fs::write(
            &path,
            "import alpha-dep\n# this should be stripped\n\
             key # whitespace comment is also stripped\nimport beta-dep\n",
        )
        .expect("write entry file");
        let refs =
            scan_entry_file_references(&path).expect("scanner must read the on-disk file");
        // Code paths register cleanly.
        assert!(refs.contains("alpha-dep"));
        assert!(refs.contains("beta-dep"));
        assert!(refs.contains("key"));
        assert!(refs.contains("import"));
        // Comment bodies must NOT be in the ref set — the
        // `in_comment` flip fires correctly on both BOL (`\n`-then-`#`)
        // and whitespace (`space`-then-`#`) precedents.
        for banned in ["this", "should", "be", "stripped", "whitespace", "comment", "is", "also"] {
            assert!(
                !refs.contains(banned),
                "comment body word `{banned}` leaked into refs (refs: {:?})",
                refs
            );
        }
    }

    /// Hash-prefixed identifier pin: `beta#suffix` is one bare
    /// identifier-like token where `#` is preceded by a
    /// non-whitespace alphanumeric char (`a` → `e` → `t` → `a`).
    /// The new gate treats this `#` as a separator (the cursor is
    /// mid-token), so tokens `beta` AND `suffix` surface — not
    /// a comment-mode flip.
    #[test]
    fn scan_entry_file_references_hash_in_bare_identifier_splits() {
        let dir = tempfile::tempdir().expect("tempdir creation");
        let path = dir.path().join("main.crush");
        std::fs::write(
            &path,
            "alpha-dep\nbeta#suffix\ngamma\ndelta#epsilon#zeta\n",
        )
        .expect("write entry file");
        let refs =
            scan_entry_file_references(&path).expect("scanner must read the on-disk file");
        // Single-`#` split: `beta` + `suffix`.
        assert!(refs.contains("alpha-dep"));
        assert!(refs.contains("beta"), "hash split must surface LHS (refs: {:?})", refs);
        assert!(refs.contains("suffix"), "hash split must surface RHS (refs: {:?})", refs);
        assert!(refs.contains("gamma"));
        // Multi-`#` split: `delta` + `epsilon` + `zeta` (chained
        // separators), confirming each `#` mid-identifier is a
        // separator, not a comment.
        assert!(refs.contains("delta"));
        assert!(refs.contains("epsilon"));
        assert!(refs.contains("zeta"));
    }

    /// BOL-at-file-start pin: a `#` at column 1 of the very first
    /// line (no preceding `\n` because the file starts there; `prev`
    /// is `None`) MUST still trigger comment mode. This exercises
    /// the third arm of the gate (`prev.is_none()`), distinct from
    /// `prev == Some('\n')` covered by the post-newline lockdown
    /// above.
    #[test]
    fn scan_entry_file_references_hash_at_file_bol_strips_comment() {
        let dir = tempfile::tempdir().expect("tempdir creation");
        let path = dir.path().join("main.crush");
        std::fs::write(
            &path,
            "# sole comment at file start\nimport alpha-dep\n",
        )
        .expect("write entry file");
        let refs =
            scan_entry_file_references(&path).expect("scanner must read the on-disk file");
        assert!(refs.contains("alpha-dep"));
        assert!(refs.contains("import"));
        // The leading-line body words must NOT be in refs.
        for banned in ["sole", "comment", "at", "file", "start"] {
            assert!(
                !refs.contains(banned),
                "BOL-file comment word `{banned}` leaked (refs: {:?})",
                refs
            );
        }
    }
}
