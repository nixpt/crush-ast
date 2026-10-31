//! Parse the repository's real `.crush` corpus with this grammar.
//!
//! The grammar's own corpus (`test/corpus/`) only runs under the tree-sitter
//! CLI, which no CI job invokes — so drift against the language the frontend
//! actually accepts went unnoticed. CRUSH-207 measured ERROR nodes in 50 of the
//! 51 `examples/crush/*.crush` files while CI stayed green. This test runs the
//! same check under plain `cargo test`.

use std::path::{Path, PathBuf};

/// Files that do not parse cleanly yet, and why.
///
/// The list only ever shrinks: if an entry starts parsing cleanly the test
/// fails so the entry gets deleted, and a new entry needs a reason recorded
/// here (and a ticket, if it is a real gap).
const KNOWN_BAD: &[(&str, &str)] = &[
    (
        "dashboard.crush",
        "the frontend rejects this file anyway — it is an `// expect-error:` \
         example whose multi-line call fails at line 30 — and the grammar stops \
         on its `catch (e)`, which is not valid Crush either (the frontend's \
         `parse_try_catch` wants a bare identifier after `catch`)",
    ),
    (
        "polyglot_braces.crush",
        "case 4 puts a bare `{` inside a Python triple-quoted string; a \
         context-free grammar cannot tell that the brace is inside a string",
    ),
];

fn parser() -> tree_sitter::Parser {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_crush::LANGUAGE.into())
        .expect("the bundled grammar loads");
    parser
}

/// The first ERROR or MISSING node, depth-first.
fn first_error(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    if node.is_error() || node.is_missing() {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = first_error(child) {
            return Some(found);
        }
    }
    None
}

/// `examples/crush/*.crush`, when this checkout has them.
///
/// Absent for a published crate (the directory is outside the package), which
/// is why the tests return early rather than fail.
fn examples() -> Option<Vec<PathBuf>> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/crush");
    if !dir.is_dir() {
        return None;
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("examples/crush is readable")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "crush"))
        .collect();
    files.sort();
    Some(files)
}

/// `test_*.crush` — the grammar's own fixtures, always present.
fn fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("the crate directory is readable")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "crush")
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("test_"))
        })
        .collect();
    files.sort();
    files
}

fn describe(path: &Path, error: tree_sitter::Node<'_>) -> String {
    let position = error.start_position();
    let kind = if error.is_missing() {
        format!("MISSING {}", error.kind())
    } else {
        "ERROR".to_string()
    };
    format!(
        "{}:{}:{} {kind}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        position.row + 1,
        position.column + 1
    )
}

#[test]
fn examples_parse_without_error_nodes() {
    let Some(files) = examples() else {
        return;
    };

    let mut parser = parser();
    let mut unexpected = Vec::new();
    let mut now_clean = Vec::new();

    for path in &files {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let source = std::fs::read_to_string(path).expect("example is readable");
        let tree = parser.parse(&source, None).expect("parse completes");
        let known_bad = KNOWN_BAD.iter().any(|(file, _)| *file == name);

        match (first_error(tree.root_node()), known_bad) {
            (None, true) => now_clean.push(name),
            (Some(error), false) => unexpected.push(describe(path, error)),
            _ => {}
        }
    }

    assert!(
        unexpected.is_empty(),
        "{} example(s) have ERROR/MISSING nodes; fix the grammar or add the \
         file to KNOWN_BAD with a reason:\n  {}",
        unexpected.len(),
        unexpected.join("\n  ")
    );
    assert!(
        now_clean.is_empty(),
        "these now parse cleanly — remove them from KNOWN_BAD:\n  {}",
        now_clean.join("\n  ")
    );
}

#[test]
fn grammar_fixtures_parse_without_error_nodes() {
    let mut parser = parser();
    let mut failures = Vec::new();

    for path in fixtures() {
        let source = std::fs::read_to_string(&path).expect("fixture is readable");
        let tree = parser.parse(&source, None).expect("parse completes");
        if let Some(error) = first_error(tree.root_node()) {
            failures.push(describe(&path, error));
        }
    }

    assert!(
        failures.is_empty(),
        "grammar fixture(s) have ERROR/MISSING nodes:\n  {}",
        failures.join("\n  ")
    );
}
