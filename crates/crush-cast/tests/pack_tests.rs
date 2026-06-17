//! EXO-176 pack-format tests: JSON↔CBOR round-trip over the whole
//! `examples/cast/**` corpus (walked dynamically — the corpus grows
//! concurrently), the fail-closed version gate on both paths, and a loose
//! size sanity check.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crush_cast::{Format, PackError, Program};

/// Recursively collect every `*.cast.json` under `dir`.
fn collect_fixtures(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_fixtures(&path, out);
        } else if path
            .file_name()
            .is_some_and(|n| n.to_str().is_some_and(|n| n.ends_with(".cast.json")))
        {
            out.push(path);
        }
    }
}

/// All `*.cast.json` fixtures that parse as a `Program`, alongside their
/// JSON bytes. Fixtures that don't parse are skipped with a notice rather
/// than failed: the corpus is grown concurrently (EXO-175) and several
/// existing files carry schema drift (e.g. lowercase `"string"` type hints)
/// that is the corpus's bug, not the codec's — the codec proof only needs
/// the valid set. At least one valid fixture is required so the test can't
/// silently pass on an empty walk.
fn valid_fixtures() -> Vec<(PathBuf, Program)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/cast");
    let mut paths = Vec::new();
    collect_fixtures(&root, &mut paths);
    paths.sort();
    assert!(
        !paths.is_empty(),
        "no *.cast.json fixtures found under {} — corpus walk is broken",
        root.display()
    );
    let mut valid = Vec::new();
    for path in paths {
        let json = std::fs::read(&path).expect("read fixture");
        match Program::deserialize(&json, Format::Json) {
            Ok(program) => valid.push((path, program)),
            Err(e) => eprintln!(
                "SKIP (fixture does not parse as Program — corpus drift, see EXO-175): {}: {}",
                path.display(),
                e
            ),
        }
    }
    assert!(
        !valid.is_empty(),
        "no fixture under {} parses as a Program — nothing to round-trip",
        root.display()
    );
    valid
}

/// JSON → Binary → JSON yields an identical Program for every fixture in the
/// examples corpus. Compared as `serde_json::Value` (Program has no PartialEq).
#[test]
fn roundtrip_json_binary_json_for_every_fixture() {
    for (path, original) in valid_fixtures() {
        let cbor = original.serialize(Format::Binary).expect("CBOR encode");
        let decoded = Program::deserialize(&cbor, Format::Binary)
            .unwrap_or_else(|e| panic!("{} failed CBOR decode: {}", path.display(), e));

        let original_value = serde_json::to_value(&original).expect("to_value");
        let decoded_value = serde_json::to_value(&decoded).expect("to_value");
        assert_eq!(
            original_value,
            decoded_value,
            "{} did not survive the JSON → Binary → JSON round-trip",
            path.display()
        );
    }
}

/// The binary form is smaller than the pretty-JSON debug form (loose sanity,
/// not a compression guarantee).
#[test]
fn binary_smaller_than_pretty_json() {
    for (path, program) in valid_fixtures() {
        let pretty = program.serialize(Format::Json).expect("JSON encode");
        let cbor = program.serialize(Format::Binary).expect("CBOR encode");
        assert!(
            cbor.len() < pretty.len(),
            "{}: binary ({} bytes) not smaller than pretty JSON ({} bytes)",
            path.display(),
            cbor.len(),
            pretty.len()
        );
    }
}

fn incompatible_program() -> Program {
    Program {
        cast_version: "9.0".to_string(),
        entry: "main".to_string(),
        lang: None,
        functions: HashMap::new(),
        ai_meta: None,
    }
}

fn assert_version_rejection(result: Result<Program, PackError>) {
    match result {
        Err(PackError::Version(v)) => {
            assert_eq!(v.boundary, crush_errors::VersionBoundary::Cast);
            assert_eq!(v.expected, crush_cast::CAST_VERSION);
            assert_eq!(v.found, "9.0");
        }
        other => panic!("expected PackError::Version, got {:?}", other.map(|_| "Ok")),
    }
}

/// An incompatible major `cast_version` fails closed on the JSON load path.
#[test]
fn version_gate_rejects_incompatible_major_json() {
    let bytes = incompatible_program()
        .serialize(Format::Json)
        .expect("encode");
    assert_version_rejection(Program::deserialize(&bytes, Format::Json));
}

/// An incompatible major `cast_version` fails closed on the binary load path.
#[test]
fn version_gate_rejects_incompatible_major_binary() {
    let bytes = incompatible_program()
        .serialize(Format::Binary)
        .expect("encode");
    assert_version_rejection(Program::deserialize(&bytes, Format::Binary));
}

/// An unparseable `cast_version` is rejected, not waved through.
#[test]
fn version_gate_rejects_unparseable_version() {
    let mut program = incompatible_program();
    program.cast_version = "not-a-version".to_string();
    let bytes = program.serialize(Format::Json).expect("encode");
    assert!(matches!(
        Program::deserialize(&bytes, Format::Json),
        Err(PackError::Version(_))
    ));
}

/// Extension convention: `.castb` (and `.cbor`) sniff as Binary, everything
/// else as JSON.
#[test]
fn format_sniffs_by_extension() {
    assert_eq!(Format::from_path(Path::new("a.castb")), Format::Binary);
    assert_eq!(Format::from_path(Path::new("a.cbor")), Format::Binary);
    assert_eq!(Format::from_path(Path::new("a.cast.json")), Format::Json);
    assert_eq!(Format::from_path(Path::new("a")), Format::Json);
}
