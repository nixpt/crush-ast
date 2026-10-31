//! The shipped editor queries must compile against this grammar.
//!
//! `tree-sitter query` only runs under the CLI; an editor that loads a query
//! naming a node the grammar no longer has just silently loses highlighting.
//! This catches that under plain `cargo test` (CRUSH-366).

use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

fn language() -> tree_sitter::Language {
    tree_sitter_crush::LANGUAGE.into()
}

#[test]
fn every_query_compiles() {
    for (name, src) in [
        ("highlights", tree_sitter_crush::HIGHLIGHTS_QUERY),
        ("indents", tree_sitter_crush::INDENTS_QUERY),
        ("locals", tree_sitter_crush::LOCALS_QUERY),
        ("injections", tree_sitter_crush::INJECTIONS_QUERY),
    ] {
        Query::new(&language(), src).unwrap_or_else(|e| panic!("{name}.scm: {e}"));
    }
}

#[test]
fn highlights_capture_the_basics() {
    let src = "fn f(n: Int) -> Int {\n    return fs.read(0xFF) + 0b1\n}\n";
    let mut parser = Parser::new();
    parser.set_language(&language()).unwrap();
    let tree = parser.parse(src, None).unwrap();
    let query = Query::new(&language(), tree_sitter_crush::HIGHLIGHTS_QUERY).unwrap();
    let mut cursor = QueryCursor::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut matches = cursor.matches(&query, tree.root_node(), src.as_bytes());
    while let Some(m) = matches.next() {
        for c in m.captures {
            seen.insert(query.capture_names()[c.index as usize].to_string());
        }
    }
    for want in [
        "keyword",
        "function",
        "variable.parameter",
        "type",
        "function.builtin",
        "number",
    ] {
        assert!(seen.contains(want), "no @{want} capture; got {seen:?}");
    }
}

#[test]
fn injections_pick_the_language() {
    let src = "@python {\n    print(1)\n}\n";
    let mut parser = Parser::new();
    parser.set_language(&language()).unwrap();
    let tree = parser.parse(src, None).unwrap();
    let query = Query::new(&language(), tree_sitter_crush::INJECTIONS_QUERY).unwrap();
    let mut cursor = QueryCursor::new();
    let mut langs = Vec::new();
    let mut matches = cursor.matches(&query, tree.root_node(), src.as_bytes());
    while let Some(m) = matches.next() {
        for p in query.property_settings(m.pattern_index) {
            if &*p.key == "injection.language" {
                langs.push(p.value.as_deref().unwrap_or("").to_string());
            }
        }
    }
    assert_eq!(langs, ["python"]);
}
