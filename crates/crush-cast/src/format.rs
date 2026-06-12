use serde_json::Value;
use std::collections::BTreeMap;

/// Produce a canonical, byte-deterministic JSON representation of a CAST Program.
///
/// Guarantees:
/// - Object keys are sorted lexicographically.
/// - Indentation is 2 spaces.
/// - No trailing whitespace.
/// - Redundant default fields are elided.
pub fn canonical_form(program: &crate::Program) -> String {
    let value = serde_json::to_value(program).expect("Program serializes to JSON");
    let canonical = canonicalize_value(value, None);
    let json = serde_json::to_string_pretty(&canonical).expect("canonical Value serializes");
    strip_trailing_whitespace(&json)
}

fn canonicalize_value(value: Value, enclosing_type: Option<&str>) -> Value {
    match value {
        Value::Object(map) => {
            let type_tag: Option<String> = map
                .get("type")
                .and_then(|v| v.as_str())
                .map(|s| s.to_owned())
                .or_else(|| enclosing_type.map(|s| s.to_owned()));
            let type_tag_ref = type_tag.as_deref().or(enclosing_type);
            let mut sorted = BTreeMap::new();
            for (k, v) in map {
                if should_elide(&k, &v, type_tag_ref) {
                    continue;
                }
                sorted.insert(k, canonicalize_value(v, type_tag_ref));
            }
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(arr) => Value::Array(
            arr.into_iter()
                .map(|v| canonicalize_value(v, enclosing_type))
                .collect(),
        ),
        other => other,
    }
}

fn should_elide(field: &str, value: &Value, type_tag: Option<&str>) -> bool {
    match (field, value) {
        // Empty meta objects — safe to elide for all typed AST nodes except
        // CapabilityCall, whose meta field lacks serde(default) in the current schema.
        ("meta", Value::Object(m)) if m.is_empty() => {
            type_tag.is_some() && type_tag != Some("CapabilityCall")
        }

        // Default type hint
        ("type_hint", Value::String(s)) if s == "Any" => true,

        // Null optional fields
        ("ai_meta", Value::Null) => true,
        ("lang", Value::Null) => true,
        ("else_body", Value::Null) => true,
        ("default_value", Value::Null) => true,
        ("result_binding", Value::Null) => true,
        ("condition", Value::Null) => true,
        ("retry_condition", Value::Null) => true,
        ("expected_format", Value::Null) => true,
        ("deadline", Value::Null) => true,
        ("db_path", Value::Null) => true,
        ("alias", Value::Null) => true,

        // Null value in Return statement
        ("value", Value::Null) if type_tag == Some("Return") => true,

        // Empty arrays with serde-default
        ("variables", Value::Array(a)) if a.is_empty() => true,
        ("imports", Value::Array(a)) if a.is_empty() => true,
        ("selective", Value::Array(a)) if a.is_empty() => true,
        ("inputs", Value::Array(a)) if a.is_empty() => true,
        ("outputs", Value::Array(a)) if a.is_empty() => true,

        // Empty objects with serde-default
        ("context", Value::Object(m)) if m.is_empty() => true,
        ("metrics", Value::Object(m)) if m.is_empty() => true,
        ("parameters", Value::Object(m)) if m.is_empty() => true,

        // Zero complexity
        ("complexity", Value::Number(n)) if n.as_u64() == Some(0) => true,

        _ => false,
    }
}

fn strip_trailing_whitespace(s: &str) -> String {
    s.lines().map(str::trim_end).collect::<Vec<_>>().join("\n")
}
