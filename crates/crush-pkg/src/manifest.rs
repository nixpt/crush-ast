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
    /// Version constraint for `language`'s runtime, in buckets/pkgx semver
    /// syntax (e.g. `"3.11"`, `"^20"`, `">=23"`) — passed straight through
    /// to `buckets::resolve` as `"{language}@{runtime_version}"`. Only
    /// meaningful for `ScriptRunner` capsules (bun/node/deno/python/sona);
    /// ignored for `crush`/`native`. Unset means an unpinned "latest" spec
    /// (buckets' `constraint == VersionReq::STAR` case), same as before
    /// this field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_version: Option<String>,
    /// Catalogue category, one of [`CATEGORIES`] (CRUSH-171). Optional;
    /// unset means uncategorised. Checked by [`Manifest::validate`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Platforms the capsule claims to run on, each one of [`PLATFORMS`]
    /// (CRUSH-171). Empty means "no claim", not "nowhere".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<String>,
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

/// A `[[dependencies]]` row: exactly one of `path` (local directory) or
/// `git` (+ exactly one of `tag` / `rev` / `branch`, CRUSH-351).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Git repository URL (anything `git clone` accepts, incl. `file://`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

impl Dependency {
    /// Reject ambiguous sources: exactly one of `path` / `git`, and a git
    /// dependency carries exactly one of `tag` / `rev` / `branch`.
    pub fn validate(&self) -> anyhow::Result<()> {
        let n = &self.name;
        match (&self.path, &self.git) {
            (Some(_), Some(_)) => anyhow::bail!(
                "dependency '{n}': set either `path` or `git`, not both"
            ),
            // Neither source: legal to *parse* (`crush-pkg lint` reads such
            // rows); `Dependency::require_source` rejects it when building.
            (None, None) => {}
            (Some(_), None) => {
                if self.tag.is_some() || self.rev.is_some() || self.branch.is_some() {
                    anyhow::bail!("dependency '{n}': `tag`/`rev`/`branch` only apply to a `git` dependency");
                }
            }
            (None, Some(url)) => {
                if url.trim().is_empty() {
                    anyhow::bail!("dependency '{n}': `git` url is empty");
                }
                let refs = [&self.tag, &self.rev, &self.branch].iter().filter(|r| r.is_some()).count();
                if refs != 1 {
                    anyhow::bail!(
                        "dependency '{n}': a git dependency needs exactly one of `tag`, `rev` or `branch` (found {refs})"
                    );
                }
            }
        }
        Ok(())
    }

    /// Building needs a source: reject a row with neither `path` nor `git`
    /// (a legacy bare `version` row is skipped by the builder, as before).
    pub fn require_source(&self) -> anyhow::Result<()> {
        if self.path.is_none() && self.git.is_none() && self.version.is_none() {
            anyhow::bail!(
                "dependency '{}': needs a source, either `path = \"...\"` or `git = \"...\"`",
                self.name
            );
        }
        Ok(())
    }
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
    Script(ScriptRuntime),
}

/// Script runtime preference.
///
/// CRUSHRUN-S2: the 4-variant shape (Bun/Node/Deno/Python) is an
/// **intent-aware cap** — extending the enum is mechanical (2 places
/// per new runtime: add the variant here + add a `get_runtime_command`
/// arm in `runners.rs`) but is NOT done speculatively. When a real
/// language demand emerges (Ruby/Go-script/Julia/Perl/R/etc.), add the
/// variant AND its binary mapping. If activation is exploratory, gate
/// the new runtime behind `--strict` (the same opt-in gate CRUSHFMT-1
/// introduced for the unknown-format run path — see PR #10). Until a
/// real signal exists, the cap is acceptable; if you're tempted to add
/// a 5th variant speculatively, prefer asking first. Closes Gap 2 of
/// `.jagent/planning/tickets/CRUSHRUNNERS-1.md` (sister branch
/// `agent/buffy/CRUSHRUNNERS-1`, PR #7 — file not merged into
/// `2f2b2f5` yet, forward-looking cross-link).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ScriptRuntime {
    #[default]
    Bun,
    Node,
    Deno,
    Python,
    Sona,
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
    Sona,
    Unknown,
}

impl PayloadFormat {
    pub fn from_path(path: &std::path::Path) -> Self {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            match ext.to_lowercase().as_str() {
                // `.sno` is Sona's compiled output: CASM JSON under its own name (#365).
                // `.cvms` / `.cvm1` / `.cvm` are compiled Crush too (CVM1 assembly / bytecode).
                "casm" | "casmb" | "sno" | "cvms" | "cvm1" | "cvm" => PayloadFormat::Casm,
                "js" | "mjs" | "cjs" => PayloadFormat::JavaScript,
                "ts" | "mts" | "cts" | "tsx" => PayloadFormat::TypeScript,
                "py" => PayloadFormat::Python,
                "sn" => PayloadFormat::Sona,
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
            PayloadFormat::Sona => Some(ScriptRuntime::Sona),
            _ => None,
        }
    }
}

/// Map language string from manifest to CapsuleType.
///
/// CRUSH-SELFHOST-1-AMEND-1 HALF 2: case-insensitive dispatch. Legacy
/// `capsule_type = "Crush"` manifests reach this function via the
/// auto-migrate path in `Manifest::from_str` (which HALF 1 lowercases to
/// `"crush"`), but a future contributor might also instantiate a
/// `Manifest` directly with mixed-case `language`. Both forms must reach
/// `CapsuleType::Crush`. To keep the canonical match arms readable we
/// lowercase once here, then match the lowercase shard.
pub fn language_to_capsule_type(language: &str) -> CapsuleType {
    match language.to_ascii_lowercase().as_str() {
        "crush" => CapsuleType::Crush,
        "native" | "rust" | "c" => CapsuleType::Native,
        // Legacy "container" arm removed (CRUSHCN-1) — see validate_language() below and
        // .jagent/planning/tickets/CRUSHRUNNERS-1.md Gap 1. Falls through to `CapsuleType::Auto` and is
        // then rejected at parse time by `Manifest::from_str` via validate_language().
        "javascript" | "js" | "ts" | "typescript" | "bun" => {
            CapsuleType::Script(ScriptRuntime::Bun)
        }
        "node" | "nodejs" => CapsuleType::Script(ScriptRuntime::Node),
        "deno" => CapsuleType::Script(ScriptRuntime::Deno),
        "python" | "py" => CapsuleType::Script(ScriptRuntime::Python),
        "sona" | "sn" => CapsuleType::Script(ScriptRuntime::Sona),
        _ => CapsuleType::Auto,
    }
}

/// Strict validation for the legacy `"container"` language value.
///
/// Returns `Err` with a user-friendly migration message when `language =
/// "container"` is encountered. Passes silently for empty strings (auto-detect)
/// and all other unknown languages (which fall through to `CapsuleType::Auto`
/// at the `language_to_capsule_type` match above — unknown extensions still
/// get an honest crush-syntax error instead of being silently mis-routed).
///
/// Rationale (CRUSHCN-1): the `ContainerRunner` stub was deleted because
/// there is no roadmap signal for any of the three candidate models
/// (Docker-spawn / OCI hooks / WASI-bundle). The legacy literal `"container"`
/// now fails loudly at parse time instead of silently routing to a stub-bail
/// at run time. See `.jagent/planning/tickets/CRUSHRUNNERS-1.md` Gap 1 for the thread history
/// and re-introduction path.
/// Known `[capsule] category` values and what each one is for (CRUSH-171).
pub const CATEGORIES: &[(&str, &str)] = &[
    ("cli", "command-line tool"),
    ("library", "reusable package, mainly consumed as a dependency"),
    ("app", "end-user application (TUI, GUI or web UI)"),
    ("service", "long-running or background capsule"),
    ("game", "game"),
    ("dev-tool", "tool for writing, building or debugging code"),
    ("language", "language support: a walker, front end or plugin"),
    ("example", "demo, tutorial or teaching material"),
];

/// Known `[capsule] platforms` values (CRUSH-171). `web` is the browser
/// (crush-web's wasm VM), which only runs Crush programs.
pub const PLATFORMS: &[&str] = &["linux", "macos", "windows", "web"];

/// Category names from the older crush-capsules sketch, mapped onto
/// [`CATEGORIES`] so manifests written against it keep loading.
fn migrate_category(category: &str) -> &str {
    match category {
        "core-utility" | "system-utility" => "cli",
        "development-tool" => "dev-tool",
        "user-app" => "app",
        "system-service" => "service",
        "language-walker" => "language",
        other => other,
    }
}

pub fn validate_language(language: &str) -> anyhow::Result<()> {
    if language == "container" {
        anyhow::bail!(
            "language = \"container\" is no longer supported in crush-pkg. \
             The Container runner stub was removed (CRUSHCN-1); pick a real \
             runtime (crush / native / python / javascript / ...) or remove \
             the language field and let entry-extension detection fill it in."
        );
    }
    Ok(())
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
        "sn" => "sona",
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
        //
        // CRUSH-SELFHOST-1-AMEND-1 HALF 1: lowercase the migrated value so
        // legacy `capsule_type = "Crush"` reaches `language = "crush"`, the
        // exact shard that `language_to_capsule_type` keys on. Without this
        // the migration preserved case and the HALF 1 audit-trail test
        // (`sdk_matrix_cell_2_legacy_capsule_type_auto_migrates_to_language`)
        // failed at `assert_eq!(language, "crush")`.
        {
            let capsule = table["capsule"]
                .as_table_mut()
                .ok_or_else(|| anyhow::anyhow!("[capsule] must be a table"))?;
            if let Some(ct) = capsule.remove("capsule_type") {
                if !capsule.contains_key("language") {
                    let normalized = match ct {
                        toml::Value::String(s) => toml::Value::String(s.to_lowercase()),
                        other => other,
                    };
                    capsule.insert("language".to_string(), normalized);
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
        let mut manifest: Manifest = value
            .try_into()
            .map_err(|e| anyhow::anyhow!("Failed to parse manifest: {}", e))?;

        // Auto-detect language if not set
        if manifest.capsule.language.is_empty() {
            manifest.capsule.language = detect_language_from_entry(&manifest.capsule.entry);
        }

        // CRUSHCN-1: reject the legacy "container" stub value at parse time.
        // Any other unknown language still falls through to `CapsuleType::Auto`
        // via `language_to_capsule_type` above (no silent fallback here either).
        validate_language(&manifest.capsule.language)?;

        if let Some(category) = manifest.capsule.category.as_mut() {
            *category = migrate_category(category).to_string();
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
        for dep in &self.dependencies {
            dep.validate()?;
        }
        self.validate_catalogue()
    }

    /// `[capsule] category` / `platforms` (CRUSH-171): known values only,
    /// no duplicate platforms, and `web` only for Crush capsules.
    fn validate_catalogue(&self) -> anyhow::Result<()> {
        if let Some(category) = &self.capsule.category
            && !CATEGORIES.iter().any(|(name, _)| name == category)
        {
            let known: Vec<&str> = CATEGORIES.iter().map(|(name, _)| *name).collect();
            anyhow::bail!(
                "unknown [capsule] category \"{category}\"; expected one of: {}",
                known.join(", ")
            );
        }
        let mut seen: Vec<&str> = Vec::new();
        for platform in &self.capsule.platforms {
            if !PLATFORMS.contains(&platform.as_str()) {
                anyhow::bail!(
                    "unknown [capsule] platform \"{platform}\"; expected any of: {}",
                    PLATFORMS.join(", ")
                );
            }
            if seen.contains(&platform.as_str()) {
                anyhow::bail!("[capsule] platforms lists \"{platform}\" more than once");
            }
            seen.push(platform);
        }
        if seen.contains(&"web") {
            let lang = self.capsule.language.as_str();
            if matches!(
                language_to_capsule_type(lang),
                CapsuleType::Native | CapsuleType::Script(_)
            ) {
                anyhow::bail!(
                    "[capsule] platforms includes \"web\", but the browser only runs Crush \
                     programs and this capsule's language is \"{lang}\""
                );
            }
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

/// The standard package layout (what `crush-pkg new` and `squeeze new`
/// generate):
///
/// ```text
/// NAME/
///   capsule.toml     name, version, entry = "src/main.crush", [[dependencies]], [capabilities]
///   capsule.lock     pinned git revs, written by the first build (commit it)
///   src/main.crush   fn main(); `import cli` → src/cli.crush, `import util.strings` → src/util/strings.crush
///   tests/*.crush    one program per file; passes when it exits 0 (`squeeze test`)
///   target/          build output (in .gitignore)
/// ```
///
/// Existing files are never overwritten (scaffolding into a directory that
/// already holds a `src/main.crush` keeps it).
pub fn scaffold_package(dir: &Path, name: &str) -> anyhow::Result<Manifest> {
    let own = crate::builder::import_name(name);
    write_if_absent(
        &dir.join("src/main.crush"),
        "fn greeting() {\n    return \"hello from Crush\"\n}\n\nfn main() {\n    io.print(greeting())\n}\n",
    )?;
    write_if_absent(
        &dir.join("tests/greeting.crush"),
        &format!(
            "// A test passes when it exits 0; `squeeze test` runs every tests/*.crush.\n\
             // The package itself (`{own}`, from src/) and its dependencies are importable.\n\
             import {own} {{ greeting }}\n\n\
             fn main() {{\n\
             \x20   if greeting() != \"hello from Crush\" {{\n\
             \x20       io.print(\"unexpected greeting: \" + greeting())\n\
             \x20       sys.exit(1)\n\
             \x20   }}\n\
             }}\n"
        ),
    )?;
    let manifest = standard_manifest(name, "src/main.crush", None);
    manifest.write_to_dir(dir)?;
    write_if_absent(&dir.join(".gitignore"), "target/\n")?;
    Ok(manifest)
}

/// [`scaffold_package`] for a library: `src/lib.crush` with no `main`,
/// category `library`, and a test that imports it by name.
pub fn scaffold_lib_package(dir: &Path, name: &str) -> anyhow::Result<Manifest> {
    let own = crate::builder::import_name(name);
    write_if_absent(
        &dir.join("src/lib.crush"),
        &format!(
            "// {name} \u{2014} a Crush library. Other packages depend on it in their\n\
             // capsule.toml and `import {own} {{ greet }}`.\n\n\
             fn greet(who) {{\n    return \"hello, \" + who\n}}\n"
        ),
    )?;
    write_if_absent(
        &dir.join("tests/greet.crush"),
        &format!(
            "// A test passes when it exits 0; `squeeze test` runs every tests/*.crush.\n\
             import {own} {{ greet }}\n\n\
             fn main() {{\n\
             \x20   if greet(\"world\") != \"hello, world\" {{\n\
             \x20       io.print(\"unexpected: \" + greet(\"world\"))\n\
             \x20       sys.exit(1)\n\
             \x20   }}\n\
             }}\n"
        ),
    )?;
    let manifest = standard_manifest(name, "src/lib.crush", Some("library"));
    manifest.write_to_dir(dir)?;
    write_if_absent(&dir.join(".gitignore"), "target/\n")?;
    Ok(manifest)
}

fn write_if_absent(path: &Path, content: &str) -> anyhow::Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(())
}

fn standard_manifest(name: &str, entry: &str, category: Option<&str>) -> Manifest {
    Manifest {
        capsule: CapsuleSection {
            name: name.to_string(),
            version: default_version(),
            description: Some(format!("{} — a Crush program", name)),
            author: None,
            entry: entry.to_string(),
            language: "crush".to_string(),
            runtime_version: None,
            category: category.map(str::to_string),
            platforms: Vec::new(),
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
    }
}

/// facet, the CLI library `crush-pkg new --cli` depends on.
pub const FACET_GIT_URL: &str = "https://github.com/nixpt/facet";
/// The facet release the scaffold pins.
pub const FACET_TAG: &str = "v0.2.0";

/// Where a `--cli` scaffold gets facet from.
#[derive(Debug, Clone)]
pub enum FacetSource {
    /// The published git tag ([`FACET_GIT_URL`] @ [`FACET_TAG`]).
    Published,
    /// A local checkout (`--facet-path DIR`), as a path dependency.
    Path(PathBuf),
}

/// Scaffold a runnable CLI program (CRUSH-352): a `capsule.toml` with category
/// `cli`, the capabilities facet needs and facet as a dependency, plus a
/// `main.crush` whose spec has a flag, a typed option with a default, a
/// positional and the automatic `--help`.
pub fn scaffold_cli_package(dir: &Path, name: &str, facet: &FacetSource) -> anyhow::Result<()> {
    if name.is_empty() || name.chars().any(|c| c == '"' || c == '\\' || c.is_control()) {
        anyhow::bail!("invalid package name {name:?}: no quotes, backslashes or control characters");
    }
    let dep = match facet {
        FacetSource::Published => format!("git = \"{FACET_GIT_URL}\"\ntag = \"{FACET_TAG}\""),
        FacetSource::Path(p) => {
            if !p.join("capsule.toml").is_file() {
                anyhow::bail!("--facet-path {}: no capsule.toml there (expected a facet checkout)", p.display());
            }
            let abs = std::fs::canonicalize(p)?;
            format!("path = \"{}\"", abs.display())
        }
    };
    let own = crate::builder::import_name(name);
    std::fs::create_dir_all(dir)?;
    std::fs::write(
        dir.join("capsule.toml"),
        format!(
            "[capsule]\nname = \"{name}\"\nversion = \"0.1.0\"\ndescription = \"{name} \u{2014} a Crush command-line program\"\n\
             entry = \"src/main.crush\"\nlanguage = \"crush\"\ncategory = \"cli\"\n\n\
             [capabilities]\nrequired = [\"io.print\", \"sys.args\", \"sys.exit\"]\n\n\
             # facet parses the command line from the spec in src/main.crush.\n\
             [[dependencies]]\nname = \"facet\"\n{dep}\n"
        ),
    )?;
    write_if_absent(
        &dir.join("src/main.crush"),
        &format!(
            "// {name} \u{2014} a command-line program. facet (a dependency, see capsule.toml)\n\
             // parses the command line from `spec`: defaults, errors (exit 2), --help.\n\
             //   squeeze run -- --help          (or: crush-pkg run -- --help)\n\
             //   squeeze run -- -v --repeat 2 hello world\n\
             import facet {{ facet_run }}\n\n\
             fn main() {{\n\
             \x20   let spec = {{ name: \"{name}\", usage: \"{name} [options] [words...]\", flags: [ {{ short: \"-v\", long: \"--verbose\", help: \"print extra detail\" }} ], options: [ {{ long: \"--repeat\", short: \"-r\", type: \"int\", default: 1, help: \"how many times to print\" }} ], positionals: [ {{ name: \"words\", repeat: true, help: \"what to print\" }} ] }}\n\
             \x20   facet_run(spec, {{ main: |args| => run(args) }})\n\
             }}\n\n\
             fn run(args) {{\n\
             \x20   let line = render(args)\n\
             \x20   let i = 0\n\
             \x20   while i < args.repeat {{\n\
             \x20       io.print(line)\n\
             \x20       i = i + 1\n\
             \x20   }}\n\
             \x20   if args.verbose == true {{ io.print(\"(repeat=\" + conv.to_str(args.repeat) + \")\") }}\n\
             }}\n\n\
             /// The line to print for the parsed arguments.\n\
             fn render(args) {{\n\
             \x20   if len(args.words) > 0 {{ return str.join(args.words, \" \") }}\n\
             \x20   return \"hello from {name}\"\n\
             }}\n"
        ),
    )?;
    write_if_absent(
        &dir.join("tests/render.crush"),
        &format!(
            "// A test passes when it exits 0; `squeeze test` runs every tests/*.crush.\n\
             import {own} {{ render }}\n\n\
             fn main() {{\n\
             \x20   if render({{ words: [\"a\", \"b\"], repeat: 1, verbose: false }}) != \"a b\" {{ sys.exit(1) }}\n\
             \x20   if render({{ words: [], repeat: 1, verbose: false }}) != \"hello from {name}\" {{ sys.exit(1) }}\n\
             }}\n"
        ),
    )?;
    write_if_absent(&dir.join(".gitignore"), "target/\n")?;
    Ok(())
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
        let err = Manifest::from_str(toml, Path::new(""))
            .unwrap_err()
            .to_string();
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
                runtime_version: None,
                category: None,
                platforms: Vec::new(),
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

    /// CRUSH-SELFHOST-1-AMEND-1 HALF 1 unit test.
    ///
    /// Paired with the integration lockstep test in
    /// `crush-pkg/tests/test_selfhost_sdk_matrix.rs::sdk_matrix_cell_2_legacy_capsule_type_auto_migrates_to_language`,
    /// this asserts the legacy `capsule_type = "Crush"` field is
    /// auto-migrated to `language = "crush"` (lowercase, the shard the
    /// `language_to_capsule_type` dispatch arm keys on).
    ///
    /// The integration test catches this end-to-end via a tempdir
    /// fixture; this unit test fires earlier (no filesystem I/O) so a
    /// regression in `Manifest::from_str`'s auto-migrate block surfaces
    /// as a single failing unit test instead of a failing integration
    /// suite, and points the fixer at the exact line in
    /// `Manifest::from_str` (see the HALF 1 block comment at the top of
    /// that section).
    #[test]
    fn auto_migrate_capsule_type_lowercases_value() {
        let toml = r#"[capsule]
name = "legacy-ct"
capsule_type = "Crush"
"#;
        let m = Manifest::from_str(toml, Path::new("")).unwrap();
        assert_eq!(
            m.capsule.language, "crush",
            "HALF 1 (auto-migrate case): legacy `capsule_type = \"Crush\"` \
             must migrate to `language = \"crush\"` (lowercase, the shard \
             the dispatcher keys on). If this assertion fails, the \
             auto-migrate block in `Manifest::from_str` no longer \
             normalises case -- see the HALF 1 doc-comment at the top \
             of that block for the lockstep-fix rationale."
        );
    }

    /// Companion unit test for `language_to_capsule_type` (HALF 2).
    /// Asserts the case-insensitive dispatch invariant contractually:
    /// `"Crush"`, `"CRUSH"`, and `"crush"` all reach `CapsuleType::Crush`.
    /// Pairs with the integration matrix test's HALF 2 assertion to lock
    /// the case-insensitivity invariant at both tiers.
    #[test]
    fn language_to_capsule_type_is_case_insensitive() {
        assert_eq!(language_to_capsule_type("crush"), CapsuleType::Crush);
        assert_eq!(language_to_capsule_type("Crush"), CapsuleType::Crush);
        assert_eq!(language_to_capsule_type("CRUSH"), CapsuleType::Crush);
        assert_eq!(language_to_capsule_type("cRuSh"), CapsuleType::Crush);
        // Sanity: same invariant for other script shards.
        assert_eq!(
            language_to_capsule_type("TypeScript"),
            CapsuleType::Script(ScriptRuntime::Bun)
        );
        assert_eq!(
            language_to_capsule_type("PYTHON"),
            CapsuleType::Script(ScriptRuntime::Python)
        );
    }

    #[test]
    fn language_container_string_rejected_at_parse() {
        let toml = r#"[capsule]
name = "legacy-container"
entry = "Dockerfile"
language = "container"
"#;
        let err = Manifest::from_str(toml, Path::new(""))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("no longer supported"),
            "expected hard-error message containing 'no longer supported', got: {}",
            err
        );
    }

    #[test]
    fn language_unknown_string_still_falls_through_to_auto() {
        let toml = r#"[capsule]
name = "future-lang"
entry = "main.crush"
language = "rust-script"
"#;
        let m = Manifest::from_str(toml, Path::new("")).unwrap();
        assert_eq!(
            language_to_capsule_type(&m.capsule.language),
            CapsuleType::Auto
        );
    }

    // ─── CRUSH-171: category / platforms ──────────────────────────────

    fn parse(capsule_extra: &str) -> anyhow::Result<Manifest> {
        let content = format!(
            "[capsule]\nname = \"x\"\nentry = \"main.crush\"\nlanguage = \"crush\"\n{capsule_extra}"
        );
        Manifest::from_str(&content, Path::new("capsule.toml"))
    }

    #[test]
    fn catalogue_fields_are_optional() {
        let m = parse("").unwrap();
        assert_eq!(m.capsule.category, None);
        assert!(m.capsule.platforms.is_empty());
        let toml = m.to_toml_string().unwrap();
        assert!(!toml.contains("category") && !toml.contains("platforms"), "{toml}");
    }

    #[test]
    fn catalogue_fields_parse_and_round_trip() {
        let m = parse("category = \"game\"\nplatforms = [\"linux\", \"web\"]\n").unwrap();
        assert_eq!(m.capsule.category.as_deref(), Some("game"));
        assert_eq!(m.capsule.platforms, vec!["linux", "web"]);
        let toml = m.to_toml_string().unwrap();
        assert!(toml.contains("category = \"game\""), "{toml}");
        let back = Manifest::from_str(&toml, Path::new("capsule.toml")).unwrap();
        assert_eq!(back.capsule.category.as_deref(), Some("game"));
        assert_eq!(back.capsule.platforms, vec!["linux", "web"]);
    }

    #[test]
    fn every_known_category_and_platform_is_accepted() {
        for (name, _) in CATEGORIES {
            parse(&format!("category = \"{name}\"\n")).unwrap();
        }
        let all = PLATFORMS.iter().map(|p| format!("\"{p}\"")).collect::<Vec<_>>().join(", ");
        parse(&format!("platforms = [{all}]\n")).unwrap();
    }

    #[test]
    fn unknown_category_is_a_clear_error() {
        let err = parse("category = \"utility\"\n").unwrap_err().to_string();
        assert!(err.contains("unknown [capsule] category \"utility\""), "{err}");
        assert!(err.contains("cli, library, app"), "{err}");
    }

    #[test]
    fn unknown_or_duplicate_platform_is_a_clear_error() {
        let err = parse("platforms = [\"linux\", \"beos\"]\n").unwrap_err().to_string();
        assert!(err.contains("unknown [capsule] platform \"beos\""), "{err}");
        assert!(err.contains("linux, macos, windows, web"), "{err}");
        let err = parse("platforms = [\"linux\", \"linux\"]\n").unwrap_err().to_string();
        assert!(err.contains("\"linux\" more than once"), "{err}");
    }

    #[test]
    fn web_platform_is_crush_only() {
        let content = "[capsule]\nname = \"x\"\nentry = \"main.py\"\nlanguage = \"python\"\nplatforms = [\"web\"]\n";
        let err = Manifest::from_str(content, Path::new("capsule.toml")).unwrap_err().to_string();
        assert!(err.contains("browser only runs Crush"), "{err}");
        assert!(err.contains("\"python\""), "{err}");
        // A Python capsule may still claim the desktop platforms.
        let ok = content.replace("[\"web\"]", "[\"linux\", \"macos\"]");
        Manifest::from_str(&ok, Path::new("capsule.toml")).unwrap();
    }

    #[test]
    fn legacy_sketch_categories_migrate() {
        for (old, new) in [
            ("core-utility", "cli"),
            ("system-utility", "cli"),
            ("development-tool", "dev-tool"),
            ("user-app", "app"),
            ("system-service", "service"),
            ("language-walker", "language"),
        ] {
            let m = parse(&format!("category = \"{old}\"\n")).unwrap();
            assert_eq!(m.capsule.category.as_deref(), Some(new), "{old}");
        }
    }

    #[test]
    fn top_level_platform_table_from_the_sketch_is_ignored() {
        // The crush-capsules sketch used a top-level `[platform]` table; it
        // doesn't collide with `[capsule] platforms` and still loads.
        let m = parse("\n[platform]\nprimary = [\"desktop\"]\n").unwrap();
        assert!(m.capsule.platforms.is_empty());
    }
}
