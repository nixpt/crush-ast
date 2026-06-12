use serde_json::Value;
use std::collections::BTreeMap;

/// Produce a human-readable semantic diff between two CAST programs.
///
/// The comparison operates on normalized AST JSON, so cosmetic differences such
/// as object key order, whitespace, and serde-default fields do not produce
/// changes. Returned lines are stable and suitable for use by a git external
/// diff driver.
pub fn diff_programs(left: &crate::Program, right: &crate::Program) -> Vec<String> {
    let left = semantic_value(left);
    let right = semantic_value(right);
    let mut changes = Vec::new();
    diff_values(&left, &right, "$", &mut changes);
    changes
}

/// Convert a CAST program to the normalized JSON value used for semantic diff.
pub fn semantic_value(program: &crate::Program) -> Value {
    let value = serde_json::to_value(program).expect("Program serializes to JSON");
    canonicalize_value(value, None)
}

fn diff_values(left: &Value, right: &Value, path: &str, changes: &mut Vec<String>) {
    match (left, right) {
        (Value::Object(left_map), Value::Object(right_map)) => {
            for (key, left_value) in left_map {
                let child_path = object_path(path, key);
                match right_map.get(key) {
                    Some(right_value) => diff_values(left_value, right_value, &child_path, changes),
                    None => changes.push(format!(
                        "- {} at {}",
                        describe_value(left_value),
                        child_path
                    )),
                }
            }

            for (key, right_value) in right_map {
                if !left_map.contains_key(key) {
                    let child_path = object_path(path, key);
                    changes.push(format!(
                        "+ {} at {}",
                        describe_value(right_value),
                        child_path
                    ));
                }
            }
        }
        (Value::Array(left_items), Value::Array(right_items)) => {
            let prefix_len = common_prefix_len(left_items, right_items);
            for index in 0..prefix_len {
                let child_path = format!("{}[{}]", path, index);
                diff_values(
                    &left_items[index],
                    &right_items[index],
                    &child_path,
                    changes,
                );
            }

            let suffix_len = common_suffix_len(left_items, right_items, prefix_len);
            let left_mid_end = left_items.len() - suffix_len;
            let right_mid_end = right_items.len() - suffix_len;
            let shared_mid = (left_mid_end - prefix_len).min(right_mid_end - prefix_len);

            for offset in 0..shared_mid {
                let left_index = prefix_len + offset;
                let right_index = prefix_len + offset;
                let child_path = format!("{}[{}]", path, right_index);
                diff_values(
                    &left_items[left_index],
                    &right_items[right_index],
                    &child_path,
                    changes,
                );
            }

            for index in prefix_len + shared_mid..left_mid_end {
                let child_path = format!("{}[{}]", path, index);
                changes.push(format!(
                    "- {} at {}",
                    describe_value(&left_items[index]),
                    child_path
                ));
            }

            for index in prefix_len + shared_mid..right_mid_end {
                let child_path = format!("{}[{}]", path, index);
                changes.push(format!(
                    "+ {} at {}",
                    describe_value(&right_items[index]),
                    child_path
                ));
            }

            for offset in 0..suffix_len {
                let left_index = left_mid_end + offset;
                let right_index = right_mid_end + offset;
                let child_path = format!("{}[{}]", path, right_index);
                diff_values(
                    &left_items[left_index],
                    &right_items[right_index],
                    &child_path,
                    changes,
                );
            }
        }
        _ if left == right => {}
        _ => changes.push(format!(
            "~ {}: {} -> {}",
            path,
            describe_scalar(left),
            describe_scalar(right)
        )),
    }
}

fn common_prefix_len(left: &[Value], right: &[Value]) -> usize {
    left.iter()
        .zip(right.iter())
        .take_while(|(left, right)| left == right)
        .count()
}

fn common_suffix_len(left: &[Value], right: &[Value], prefix_len: usize) -> usize {
    let max_suffix = left.len().min(right.len()).saturating_sub(prefix_len);
    left.iter()
        .rev()
        .zip(right.iter().rev())
        .take(max_suffix)
        .take_while(|(left, right)| left == right)
        .count()
}

fn canonicalize_value(value: Value, enclosing_type: Option<&str>) -> Value {
    match value {
        Value::Object(map) => {
            let type_tag = map
                .get("type")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .or_else(|| enclosing_type.map(str::to_owned));
            let type_tag_ref = type_tag.as_deref().or(enclosing_type);
            let mut sorted = BTreeMap::new();
            for (key, value) in map {
                if should_elide(&key, &value, type_tag_ref) {
                    continue;
                }
                sorted.insert(key, canonicalize_value(value, type_tag_ref));
            }
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|value| canonicalize_value(value, enclosing_type))
                .collect(),
        ),
        other => other,
    }
}

fn should_elide(field: &str, value: &Value, type_tag: Option<&str>) -> bool {
    match (field, value) {
        ("meta", Value::Object(map)) if map.is_empty() => {
            type_tag.is_some() && type_tag != Some("CapabilityCall")
        }
        ("type_hint", Value::String(s)) if s == "Any" => true,
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
        ("value", Value::Null) if type_tag == Some("Return") => true,
        ("variables", Value::Array(items)) if items.is_empty() => true,
        ("imports", Value::Array(items)) if items.is_empty() => true,
        ("selective", Value::Array(items)) if items.is_empty() => true,
        ("inputs", Value::Array(items)) if items.is_empty() => true,
        ("outputs", Value::Array(items)) if items.is_empty() => true,
        ("context", Value::Object(map)) if map.is_empty() => true,
        ("metrics", Value::Object(map)) if map.is_empty() => true,
        ("parameters", Value::Object(map)) if map.is_empty() => true,
        ("complexity", Value::Number(n)) if n.as_u64() == Some(0) => true,
        _ => false,
    }
}

fn object_path(parent: &str, key: &str) -> String {
    if parent == "$" {
        key.to_string()
    } else if is_identifier(key) {
        format!("{}.{}", parent, key)
    } else {
        format!(
            "{}[{}]",
            parent,
            serde_json::to_string(key).expect("key serializes")
        )
    }
}

fn is_identifier(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn describe_value(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            if let Some(type_name) = map.get("type").and_then(Value::as_str) {
                let mut fields = Vec::new();
                for key in ["name", "function", "operator", "field", "lang", "variable"] {
                    if let Some(value) = map.get(key) {
                        fields.push(format!("{}: {}", key, describe_scalar(value)));
                    }
                }

                if fields.is_empty() {
                    format!("{} {{ ... }}", typed_name(type_name))
                } else {
                    format!("{} {{ {} }}", typed_name(type_name), fields.join(", "))
                }
            } else {
                format!("object with {} keys", map.len())
            }
        }
        Value::Array(items) => format!("array with {} items", items.len()),
        other => describe_scalar(other),
    }
}

fn typed_name(type_name: &str) -> String {
    match type_name {
        "VarDecl" | "Export" | "ExprStmt" | "If" | "While" | "For" | "Return" | "TryCatch"
        | "Throw" | "FunctionDef" | "SetField" | "LangBlock" | "Import" | "StructDef" | "Break"
        | "Continue" | "DomMutate" | "DomEventListener" => {
            format!("Statement::{}", type_name)
        }
        _ => type_name.to_string(),
    }
}

fn describe_scalar(value: &Value) -> String {
    match value {
        Value::String(s) => serde_json::to_string(s).expect("string serializes"),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        other => describe_value(other),
    }
}
