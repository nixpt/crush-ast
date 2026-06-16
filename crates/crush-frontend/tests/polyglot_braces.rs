use crush_cast::Statement;
use crush_frontend::parse_source;
use std::path::PathBuf;

fn first_lang_block(source: &str) -> (String, String) {
    let program = parse_source(source).expect("polyglot block should parse");
    let main = program.functions.get("main").expect("main function");
    for stmt in &main.body {
        if let Statement::LangBlock { lang, code, .. } = stmt {
            return (lang.clone(), code.clone());
        }
    }
    panic!("no LangBlock parsed from: {source:?}");
}

#[test]
fn python_block_with_fstring_brace_inside_string_does_not_close_body() {
    let src = "@python {\nprint(f\"value = {x}\")\n}\n";
    let (lang, code) = first_lang_block(src);
    assert_eq!(lang, "python");
    assert!(
        code.contains("f\"value = {x}\""),
        "f-string body lost: {code:?}"
    );
}

#[test]
fn javascript_block_with_nested_object_literal_on_single_line() {
    let src = "@javascript { const o = {a: 1, b: {c: 2}}; }\n";
    let (lang, code) = first_lang_block(src);
    assert_eq!(lang, "javascript");
    assert!(
        code.contains("{a: 1, b: {c: 2}}"),
        "nested object literal lost: {code:?}"
    );
}

#[test]
fn javascript_block_with_backtick_template_literal() {
    let src = "@javascript {\nconst s = `x = `\n}\n";
    let (lang, code) = first_lang_block(src);
    assert_eq!(lang, "javascript");
    assert!(code.contains("`x = `"), "template literal lost: {code:?}");
}

#[test]
fn python_block_with_triple_quoted_brace_on_its_own_line() {
    let src = "@python {\ns = \"\"\"\n{\nline2\n\"\"\"\n}\n";
    let (lang, code) = first_lang_block(src);
    assert_eq!(lang, "python");
    assert!(
        code.contains("\"\"\"\n{\nline2\n\"\"\""),
        "triple-quoted body lost: {code:?}"
    );
}

#[test]
fn existing_simple_python_block_still_parses() {
    // Mirrors tests/language/lang_test.crush so we don't regress the
    // flush-left close-brace convention.
    let src = "@python {\nimport math\nprint(\"Python Pi:\", math.pi)\n}\n";
    let (lang, code) = first_lang_block(src);
    assert_eq!(lang, "python");
    assert!(code.contains("import math"));
}

#[test]
fn workspace_polyglot_braces_crush_fixture_parses_cleanly() {
    // The dispatch's deliverable test file lives at the workspace level
    // because the polyglot-block surface is exercised across walkers.
    // We locate it via CARGO_MANIFEST_DIR so the test is path-stable.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = manifest
        .ancestors()
        .find_map(|p| {
            let candidate = p.join("examples/crush/polyglot_braces.crush");
            candidate.exists().then_some(candidate)
        })
        .expect("polyglot_braces.crush fixture missing (expected at examples/crush/)");

    let source = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|e| panic!("read {}: {e}", fixture.display()));

    let program = parse_source(&source)
        .unwrap_or_else(|e| panic!("fixture {} failed to parse: {e}", fixture.display()));

    let main = program.functions.get("main").expect("main function");
    let lang_blocks = main
        .body
        .iter()
        .filter(|s| matches!(s, Statement::LangBlock { .. }))
        .count();
    assert!(
        lang_blocks >= 4,
        "expected at least 4 polyglot blocks in fixture, got {lang_blocks}"
    );
}
