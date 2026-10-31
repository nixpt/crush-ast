//! The Crush file formats, as the CLIs read and name them (#365).
//!
//! One place decides what a path holds, so `crushc`, `crush-run`,
//! `crush-compile` and `crush-pkg` agree. The table of extensions, encodings,
//! magic/version constants and producers is `docs/FILE-FORMATS.md`.
//!
//! | Extension | Holds |
//! |---|---|
//! | `.crush` | Crush source |
//! | `.cast.json` (`.cast`) | CAST IR, JSON |
//! | `.castb` (`.cbor`) | CAST IR, CBOR |
//! | `.casm` (`.sno`) | CASM IR, JSON (`casm::Program`) |
//! | `.casmb` | CASM IR, MessagePack (shebang line first) |
//! | `.cvms` | CVM1 assembly text |
//! | `.cvm1` (`.cvm`) | CVM1 bytecode (magic `CVM1`) |
//!
//! Deprecated for one release, accepted with a note on stderr: `.casm.json`
//! (CASM JSON, now `.casm`) and a `.casm` file holding CVM1 assembly text
//! (first non-blank byte isn't `{`; now `.cvms`).

use std::path::{Path, PathBuf};

/// What a file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputFormat {
    /// Crush source (`.crush`).
    Crush,
    /// CAST IR as JSON (`.cast.json`, `.cast`).
    CastJson,
    /// CAST IR as CBOR (`.castb`, `.cbor`).
    CastBinary,
    /// CASM IR as JSON (`.casm`, `.sno`, deprecated `.casm.json`).
    CasmJson,
    /// CASM IR as MessagePack (`.casmb`).
    CasmBinary,
    /// CVM1 assembly text (`.cvms`; a text `.casm`, deprecated).
    Cvms,
    /// CVM1 bytecode (`.cvm1`, `.cvm`).
    Cvm1,
}

impl InputFormat {
    /// Whether this format is CAST (either encoding).
    pub fn is_cast(self) -> bool {
        matches!(self, Self::CastJson | Self::CastBinary)
    }

    /// Whether this format is CASM IR (either encoding).
    pub fn is_casm(self) -> bool {
        matches!(self, Self::CasmJson | Self::CasmBinary)
    }
}

/// A detected format plus the deprecation note to print, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detected {
    pub format: InputFormat,
    /// `Some` when the path uses a name accepted only for this release.
    pub note: Option<String>,
}

/// The extensions every runner accepts, for error messages.
pub const ACCEPTED: &str =
    ".crush, .cast.json / .cast / .castb, .casm / .casmb / .sno, .cvms, or .cvm1 / .cvm";

/// The lower-cased file name of `path`.
fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

/// Whether `bytes` starts (after whitespace) with `{`: JSON, not assembly text.
fn looks_like_json(bytes: &[u8]) -> bool {
    bytes.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{')
}

/// What `path` holds, judged by its name (and, for `.casm`, by its first
/// non-blank byte). `None` for a name no tool reads.
pub fn detect(path: &Path) -> anyhow::Result<Option<Detected>> {
    let name = file_name(path);
    let plain = |format| Ok(Some(Detected { format, note: None }));
    if name.ends_with(".cast.json") {
        return plain(InputFormat::CastJson);
    }
    if name.ends_with(".casm.json") {
        return Ok(Some(Detected {
            format: InputFormat::CasmJson,
            note: Some(format!(
                "note: {}: `.casm.json` is deprecated — CASM JSON is `.casm` now; rename it \
                 (`.casm.json` is accepted for one more release)",
                path.display()
            )),
        }));
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    match ext {
        "crush" => plain(InputFormat::Crush),
        "cast" => plain(InputFormat::CastJson),
        "castb" | "cbor" => plain(InputFormat::CastBinary),
        "sno" => plain(InputFormat::CasmJson),
        "casmb" => plain(InputFormat::CasmBinary),
        "cvms" => plain(InputFormat::Cvms),
        "cvm1" | "cvm" => plain(InputFormat::Cvm1),
        "casm" => {
            let bytes = std::fs::read(path)
                .map_err(|e| anyhow::anyhow!("cannot read '{}': {e}", path.display()))?;
            if looks_like_json(&bytes) {
                plain(InputFormat::CasmJson)
            } else {
                Ok(Some(Detected {
                    format: InputFormat::Cvms,
                    note: Some(format!(
                        "note: {}: this `.casm` holds CVM1 assembly text — `.casm` means CASM \
                         JSON now; rename it to `.cvms` (text `.casm` is accepted for one more \
                         release)",
                        path.display()
                    )),
                }))
            }
        }
        _ => Ok(None),
    }
}

/// [`detect`], or an error naming the accepted extensions. With `show_note`
/// the deprecation note (if any) goes to stderr; callers in
/// `--message-format json` mode pass `false` so stderr stays pure NDJSON.
pub fn detect_or_bail(path: &Path, show_note: bool) -> anyhow::Result<InputFormat> {
    match detect(path)? {
        Some(d) => {
            if let (Some(note), true) = (&d.note, show_note) {
                eprintln!("{note}");
            }
            Ok(d.format)
        }
        None => {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            anyhow::bail!("unsupported file extension: {ext} (expected {ACCEPTED})")
        }
    }
}

/// Load a CAST file through the gated [`crush_cast::Program::load`] (the
/// `cast_version` major must match [`crush_cast::CAST_VERSION`]).
pub fn load_cast(path: &Path, format: InputFormat) -> anyhow::Result<crush_cast::Program> {
    let data = std::fs::read(path).map_err(|e| anyhow::anyhow!("cannot read '{}': {e}", path.display()))?;
    let codec = match format {
        InputFormat::CastBinary => crush_cast::Format::Binary,
        _ => crush_cast::Format::Json,
    };
    crush_cast::Program::deserialize(&data, codec)
        .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))
}

/// Load a CASM file through the gated `casm::Program::deserialize` (the
/// `version` major must match [`casm::CASM_VERSION`]).
pub fn load_casm(path: &Path, format: InputFormat) -> anyhow::Result<casm::Program> {
    let data = std::fs::read(path).map_err(|e| anyhow::anyhow!("cannot read '{}': {e}", path.display()))?;
    let codec = match format {
        InputFormat::CasmBinary => casm::Format::Binary,
        _ => casm::Format::Json,
    };
    casm::Program::deserialize(&data, codec).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))
}

/// `path` without its format extension: `x.cast.json` → `x`, `x.crush` → `x`.
pub fn artifact_stem(path: &Path) -> PathBuf {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let lower = name.to_ascii_lowercase();
    for compound in [".cast.json", ".casm.json"] {
        if lower.ends_with(compound) {
            return path.with_file_name(&name[..name.len() - compound.len()]);
        }
    }
    path.with_extension("")
}

/// The default artifact path for `input`: `<stem>.<ext>` beside it, where
/// `ext` may itself be compound (`cast.json`).
pub fn artifact_path(input: &Path, ext: &str) -> PathBuf {
    let stem = artifact_stem(input);
    let name = stem.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    stem.with_file_name(format!("{name}.{ext}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect_named(name: &str, body: &str) -> Option<Detected> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        std::fs::write(&path, body).unwrap();
        detect(&path).unwrap()
    }

    #[test]
    fn extensions_map_to_one_format_each() {
        let cases = [
            ("a.crush", InputFormat::Crush),
            ("a.cast.json", InputFormat::CastJson),
            ("a.cast", InputFormat::CastJson),
            ("a.castb", InputFormat::CastBinary),
            ("a.sno", InputFormat::CasmJson),
            ("a.casmb", InputFormat::CasmBinary),
            ("a.cvms", InputFormat::Cvms),
            ("a.cvm1", InputFormat::Cvm1),
            ("a.cvm", InputFormat::Cvm1),
        ];
        for (name, want) in cases {
            let d = detect_named(name, "{}").unwrap();
            assert_eq!(d.format, want, "{name}");
            assert!(d.note.is_none(), "{name}");
        }
        assert!(detect_named("a.json", "{}").is_none());
        assert!(detect_named("a.txt", "").is_none());
    }

    #[test]
    fn casm_is_json_and_a_text_casm_is_cvms_with_a_note() {
        let json = detect_named("a.casm", "  \n{\"version\":\"1.0\"}").unwrap();
        assert_eq!(json, Detected { format: InputFormat::CasmJson, note: None });
        let text = detect_named("a.casm", ".func main\nHALT\n").unwrap();
        assert_eq!(text.format, InputFormat::Cvms);
        assert!(text.note.unwrap().contains("rename it to `.cvms`"));
    }

    #[test]
    fn casm_json_is_deprecated_with_a_note() {
        let d = detect_named("a.casm.json", "{}").unwrap();
        assert_eq!(d.format, InputFormat::CasmJson);
        assert!(d.note.unwrap().contains("CASM JSON is `.casm` now"));
    }

    #[test]
    fn artifact_paths_strip_compound_extensions() {
        assert_eq!(artifact_path(Path::new("d/x.crush"), "cvm1"), Path::new("d/x.cvm1"));
        assert_eq!(artifact_path(Path::new("d/x.cast.json"), "cvm1"), Path::new("d/x.cvm1"));
        assert_eq!(artifact_path(Path::new("x.crush"), "cast.json"), Path::new("x.cast.json"));
        assert_eq!(artifact_path(Path::new("x.casm.json"), "casm"), Path::new("x.casm"));
        assert_eq!(artifact_path(Path::new("x.castb"), "cvms"), Path::new("x.cvms"));
    }
}
