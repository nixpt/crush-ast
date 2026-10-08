//! Build-then-run composition (folded in from squeeze, CRUSH-167).
//!
//! `crush-pkg build` and `crush-pkg run` are separate steps: `run`
//! interprets the entry file directly and never writes `target/`. A bare
//! `crush-pkg` composes them: build → write `target/` → run the program
//! that was just built. That flow, and `build`/`check` on their
//! own, only make sense for Crush-source capsules, so this module also
//! holds the guard that refuses Script/Native capsules with a clear
//! message instead of feeding Python or JavaScript to the Crush compiler.

use std::path::Path;

use crate::manifest::{CapsuleType, Manifest, PayloadFormat, language_to_capsule_type};

/// What the build pipeline can do with a capsule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Buildability {
    /// Crush source (or CASM): `check`/`build` apply.
    Crush,
    /// Script or native capsule: only `run` applies. Carries a short
    /// description of why, for the refusal message.
    NotCrush(String),
}

/// Classify a capsule. The manifest's `language` decides when it names a
/// known runtime; otherwise the entry file's extension (then its magic
/// bytes) decides. Anything not recognisably Script/Native counts as
/// Crush, so an unrecognised entry still gets the compiler's own error.
pub fn buildability(manifest: &Manifest, root: &Path) -> Buildability {
    let lang = manifest.capsule.language.as_str();
    match language_to_capsule_type(lang) {
        CapsuleType::Crush => return Buildability::Crush,
        CapsuleType::Native | CapsuleType::Script(_) => {
            return Buildability::NotCrush(format!("manifest declares language = \"{lang}\""));
        }
        _ => {}
    }

    let entry = root.join(&manifest.capsule.entry);
    let mut format = PayloadFormat::from_path(&entry);
    if format == PayloadFormat::Unknown
        && let Ok(bytes) = std::fs::read(&entry)
    {
        format = PayloadFormat::from_magic(&bytes);
    }
    let what = match format {
        PayloadFormat::Casm | PayloadFormat::Unknown => return Buildability::Crush,
        PayloadFormat::JavaScript => "a JavaScript",
        PayloadFormat::TypeScript => "a TypeScript",
        PayloadFormat::Python => "a Python",
        PayloadFormat::Sona => "a Sona",
        PayloadFormat::NativeElf | PayloadFormat::NativeMachO | PayloadFormat::NativePe => {
            "a native binary"
        }
    };
    Buildability::NotCrush(format!("entry `{}` is {what} file", manifest.capsule.entry))
}

/// Refuse `subcommand` (`build`, `check`) on a non-Crush capsule.
pub fn require_crush_buildable(
    manifest: &Manifest,
    root: &Path,
    subcommand: &str,
) -> anyhow::Result<()> {
    match buildability(manifest, root) {
        Buildability::Crush => Ok(()),
        Buildability::NotCrush(why) => anyhow::bail!(
            "`crush-pkg {subcommand}` only applies to Crush-source capsules ({why}); \
             use `crush-pkg run` (or a bare `crush-pkg`) to run this capsule"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::CapsuleSection;

    fn manifest(entry: &str, language: &str) -> Manifest {
        Manifest {
            capsule: CapsuleSection {
                name: "demo".into(),
                version: "0.1.0".into(),
                entry: entry.into(),
                language: language.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn crush_language_is_buildable() {
        let dir = tempfile::tempdir().unwrap();
        let m = manifest("src/main.crush", "crush");
        assert_eq!(buildability(&m, dir.path()), Buildability::Crush);
        assert!(require_crush_buildable(&m, dir.path(), "build").is_ok());
    }

    #[test]
    fn script_and_native_languages_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        for lang in ["python", "node", "bun", "deno", "sona", "native", "rust"] {
            let m = manifest("main.x", lang);
            let err = require_crush_buildable(&m, dir.path(), "check").unwrap_err();
            let msg = err.to_string();
            assert!(msg.contains("`crush-pkg check` only applies"), "{lang}: {msg}");
            assert!(msg.contains(&format!("language = \"{lang}\"")), "{lang}: {msg}");
        }
    }

    #[test]
    fn auto_language_decides_by_entry_extension() {
        let dir = tempfile::tempdir().unwrap();
        for (entry, buildable) in [
            ("main.crush", true),
            ("main.casm", true),
            ("main.py", false),
            ("main.js", false),
            ("main.ts", false),
            ("main.sn", false),
        ] {
            let m = manifest(entry, "");
            let got = buildability(&m, dir.path()) == Buildability::Crush;
            assert_eq!(got, buildable, "{entry}");
        }
    }

    #[test]
    fn auto_language_detects_native_by_magic() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("app"), [0x7F, b'E', b'L', b'F', 2, 1]).unwrap();
        let m = manifest("app", "");
        assert!(matches!(
            buildability(&m, dir.path()),
            Buildability::NotCrush(why) if why.contains("native binary")
        ));
    }
}
