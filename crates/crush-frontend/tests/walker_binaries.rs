//! Every walker binary the registry runs must be a binary some workspace
//! crate actually builds. `crush_lang_{go,zig,wasm}` (vs Cargo's
//! `crush-lang-*`) went unnoticed because nothing checked this.

use crush_frontend::language_walkers::WalkerRegistry;
use std::collections::BTreeSet;
use std::path::Path;

/// Binary names built by the crates under `crates/`: each `[[bin]]` name, or
/// the package name for a crate with an implicit `src/main.rs` binary.
fn workspace_binaries() -> BTreeSet<String> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut bins = BTreeSet::new();
    for entry in std::fs::read_dir(&crates).unwrap() {
        let dir = entry.unwrap().path();
        let Ok(manifest) = std::fs::read_to_string(dir.join("Cargo.toml")) else {
            continue;
        };
        let mut section = "";
        let mut explicit_bin = false;
        let mut package_name = None;
        for line in manifest.lines().map(str::trim) {
            if line.starts_with('[') {
                section = line;
                explicit_bin |= line == "[[bin]]";
            } else if let Some(value) = line.strip_prefix("name").map(str::trim) {
                let Some(name) = value.strip_prefix('=').map(|v| v.trim().trim_matches('"')) else {
                    continue;
                };
                match section {
                    "[[bin]]" => {
                        bins.insert(name.to_string());
                    }
                    "[package]" => package_name = Some(name.to_string()),
                    _ => {}
                }
            }
        }
        if !explicit_bin && dir.join("src/main.rs").is_file() {
            bins.extend(package_name);
        }
    }
    assert!(
        bins.contains("python_walker"),
        "manifest scan found nothing: {bins:?}"
    );
    bins
}

#[test]
fn every_registered_walker_binary_is_built_by_the_workspace() {
    let bins = workspace_binaries();
    let registry = WalkerRegistry::new();
    let mut languages = registry.supported_languages();
    languages.sort();
    assert!(!languages.is_empty());
    for language in languages {
        let walker = registry.get_walker(&language).unwrap();
        if let Some(binary) = walker.binary_name() {
            assert!(
                bins.contains(binary),
                "{language} walker runs `{binary}`, which no workspace crate builds (built: {bins:?})"
            );
        }
    }
}
