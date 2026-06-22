//! Helpers for the portable capabilities built into `crush-vm`.
//!
//! These functions mirror the behaviour of the VM's hard-coded capability
//! dispatch so that host code can pre-validate or post-process capability
//! calls without running a full program.

use crush_vm::vm::Value;

/// Errors from capability helper operations.
#[derive(Debug, thiserror::Error)]
pub enum CapabilityError {
    #[error("capability {cap} expects {expected} argument(s), got {got}")]
    Arity {
        cap: &'static str,
        expected: usize,
        got: usize,
    },

    #[error("capability {cap} received argument of wrong type: {detail}")]
    Type { cap: &'static str, detail: String },
}

/// Render a slice of CVM1 values as text, the same way `io.print` and
/// `str.concat` do inside the VM.
pub fn print(args: &[Value]) -> String {
    args.iter().map(value_as_text).collect::<Vec<_>>().concat()
}

/// Alias for [`print`], matching the `str.concat` capability semantics.
pub fn concat(args: &[Value]) -> String {
    print(args)
}

/// Compute the byte length of a value's text representation, matching
/// `str.len`.
pub fn len(value: &Value) -> Result<i64, CapabilityError> {
    Ok(value_as_text(value).len() as i64)
}

/// Render a single CVM1 value as text, matching the VM's `Value::as_text`.
///
/// **Delegates** to the canonical `impl Display for Value` (defined in
/// `crush_vm::vm::vm.rs`) so `io.print` / `str.concat` and the stdlib's
/// `conv.to_str` / `str.format` / `str.join` / `path.*` all render the
/// same way — there is one formatter and one only. Kept as a thin
/// wrapper for backwards compatibility with any host code that imports
/// it directly.
pub fn value_as_text(value: &crush_vm::vm::Value) -> String {
    value.to_string()
}

/// Convert a text representation back into a CVM1 value. Designed as the
/// exact canonical inverse of [`crush_vm::vm::Value`]'s `impl Display`:
/// for every variant, `text_as_value(format!("{}", v).as_str()) == v`.
///
/// Disambiguation rules (each one mirrors a property of the Display impl):
///
/// - **Canonical scalars win over `Str`**: the literals `"null"`,
///   `"true"`, `"false"`, every `i64`-parseable token, and every
///   `f64`-parseable token (including the `Value::Float(3.0)` form
///   `"3.0"`, locked by the `Display::{:.1}` suffix) are reconstructed
///   as their typed variants. Only content that does **not** match
///   any of those patterns falls through to `Value::Str`. This means
///   `text_as_value("null") == Value::Null` — the document case — and
///   that a `Str` whose contents happen to look like a canonical scalar
///   gets reinterpreted; the canonical form always wins (a documented
///   trade-off; see "Caveats").
///
/// - **Top-level-aware delimiter splitting**: nested `Array` and `Map`
///   contents are split at commas / first colons encountered at depth
///   zero. Tokens inside `[...]`, `{...}`, or `(...)` brackets are
///   never treated as delimiters, so `Value::Map` entries with
///   recursive values (`{k: [1, 2]}`, `{k: {nested: 1}}`,
///   `{k: error(oops)}`) round-trip correctly.
///
/// - **Recursion depth cap = 64**: pathological inputs (`[[[[...`)
///   cannot blow the stack; excess depth falls back to `Value::Str(s)`
///   (defensive but minimal).
///
/// - **Tagged-prefix forms**: `error(msg)`, `<N bytes>`,
///   `<handle N>` are matched by literal prefix/suffix. `Bytes`
///   round-trip caveat: Display only preserves the length, so the
///   reconstructed `Vec<u8>` is zero-filled to that length (the
///   inverse property holds under `Value::PartialEq` only for
///   all-zero Bytes payloads, documented limitation).
pub fn text_as_value(text: &str) -> Value {
    parse_value(text, 0)
}

/// Recursive core for [`text_as_value`]. Public wrap is so callers don't
/// see the depth parameter; `depth == 0` is the top-level entry.
fn parse_value(s: &str, depth: usize) -> Value {
    if depth > 64 {
        // Defensive cap on stack depth for adversarial inputs.
        return Value::Str(s.to_string());
    }

    // Canonical scalar literals take precedence over any Str fallback.
    if s == "null" {
        return Value::Null;
    }
    if s == "true" {
        return Value::Bool(true);
    }
    if s == "false" {
        return Value::Bool(false);
    }
    // Int: must precede Float — e.g. "3.0" is NOT an Int under i64::parse,
    // so this branch is safe; negative integers parse here cleanly.
    if let Ok(i) = s.parse::<i64>() {
        return Value::Int(i);
    }
    // Float: locks the `Display::{:.1}` form (e.g. "3.0" → 3.0_f64 → Float(3.0)).
    if let Ok(f) = s.parse::<f64>() {
        return Value::Float(f);
    }

    let s_trim = s.trim();

    // Value::Array inverse — `[e1, e2, ...]` (comma-space joined on Display).
    if s_trim.starts_with('[') && s_trim.ends_with(']') {
        let inner = s_trim[1..s_trim.len() - 1].trim();
        if inner.is_empty() {
            return Value::new_array(vec![]);
        }
        let parsed = split_top_level(inner, ',')
            .into_iter()
            .map(|p| parse_value(p.trim(), depth + 1))
            .collect();
        return Value::new_array(parsed);
    }

    // Value::Map inverse — `{k: v, k2: v2}` (colon-space, comma-space joined).
    if s_trim.starts_with('{') && s_trim.ends_with('}') {
        let inner = s_trim[1..s_trim.len() - 1].trim();
        if inner.is_empty() {
            return Value::new_map(std::collections::HashMap::new());
        }
        let mut map = std::collections::HashMap::new();
        for pair in split_top_level(inner, ',') {
            if let Some((k, v)) = split_first_top_level(pair.trim(), ':') {
                map.insert(k.trim().to_string(), parse_value(v.trim(), depth + 1));
            } else {
                // Malformed entry: stray token without a top-level `:`.
                // Don't try to repair — degrade to identity Str(s).
                return Value::Str(s.to_string());
            }
        }
        return Value::new_map(map);
    }

    // Value::Error inverse — `error(msg)`.
    if s.starts_with("error(") && s.ends_with(')') {
        return Value::Error(s[6..s.len() - 1].to_string());
    }

    // Value::Bytes inverse — `<N bytes>`. Display only preserves length;
    // reconstruct zero-filled Vec<u8> of that length (documented caveat).
    if s.starts_with('<') && s.ends_with(" bytes>") {
        if let Ok(n) = s[1..s.len() - 7].parse::<usize>() {
            return Value::Bytes(vec![0; n]);
        }
    }

    // Value::Handle inverse — `<handle N>`.
    if s.starts_with("<handle ") && s.ends_with('>') {
        if let Ok(n) = s[8..s.len() - 1].parse::<u64>() {
            return Value::Handle(n);
        }
    }

    Value::Str(s.to_string())
}

/// Split `s` at every top-level occurrence of `delim`, respecting nested
/// brackets `[...]`, braces `{...}`, and parentheses `(...)`. Splits
/// that land inside those brackets are skipped. Used for parsing
/// `Array` contents (`,` delim) and `Map` entries (`,` delim).
fn split_top_level(s: &str, delim: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut bracket_depth: i32 = 0;
    let mut brace_depth: i32 = 0;
    let mut paren_depth: i32 = 0;
    let mut start = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            _ if c == delim && bracket_depth == 0 && brace_depth == 0 && paren_depth == 0 => {
                parts.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

/// Like `split_top_level` but only the FIRST top-level `delim` — used
/// to peel a Map entry into its `(key, value)` halves at the first
/// top-level colon (`{k: v}` → `Some(("k", "v"))`).
fn split_first_top_level(s: &str, delim: char) -> Option<(&str, &str)> {
    let mut bracket_depth: i32 = 0;
    let mut brace_depth: i32 = 0;
    let mut paren_depth: i32 = 0;
    for (i, c) in s.char_indices() {
        match c {
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            _ if c == delim && bracket_depth == 0 && brace_depth == 0 && paren_depth == 0 => {
                return Some((&s[..i], &s[i + c.len_utf8()..]));
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_concatenates_values() {
        let args = vec![
            Value::Str("hello ".to_string()),
            Value::Int(42),
            Value::Null,
        ];
        assert_eq!(print(&args), "hello 42null");
        assert_eq!(concat(&args), "hello 42null");
    }

    #[test]
    fn len_counts_bytes() {
        assert_eq!(len(&Value::Str("abc".to_string())).unwrap(), 3);
        assert_eq!(len(&Value::Int(12345)).unwrap(), 5);
    }

    #[test]
    fn float_without_fraction_has_decimal() {
        assert_eq!(value_as_text(&Value::Float(3.0)), "3.0");
    }

    #[test]
    fn text_as_value_roundtrip() {
        assert_eq!(text_as_value("null"), Value::Null);
        assert_eq!(text_as_value("42"), Value::Int(42));
        assert_eq!(text_as_value("3.14"), Value::Float(3.14));
        assert_eq!(text_as_value("foo"), Value::Str("foo".to_string()));
    }

    // ── Edge-case fixtures for canonical `Text` parser (`text_as_value`).
    // `text_as_value ∘ Display == id` for every variant is locked by
    // `crush-vm`'s `all_traits_round_trip_for_every_variant` matrix —
    // the source-of-truth pivots here. The fixtures below cover the
    // cross-crate concerns that the matrix can't see directly (it
    // round-trips through Display, so bracket/parse-only inputs are
    // out of scope) and a couple of bracket-edge cases that exercise
    // the canonical `split_top_level` / `split_first_top_level`
    // helpers.

    #[test]
    fn text_as_value_edge_cases() {
        // Nested Array — `[1, 2], [3]`.
        assert_eq!(
            text_as_value("[[1, 2], [3]]"),
            Value::new_array(vec![
                Value::new_array(vec![Value::Int(1), Value::Int(2)]),
                Value::new_array(vec![Value::Int(3)]),
            ])
        );

        // Nested Map — `{k: {nested: 1}}`. Top-level `:` for the outer pair
        // is found before the inner `{nested: 1}` (recursive parse handles it).
        let mut inner = std::collections::HashMap::new();
        inner.insert("nested".to_string(), Value::Int(1));
        let mut outer = std::collections::HashMap::new();
        outer.insert("k".to_string(), Value::new_map(inner));
        assert_eq!(text_as_value("{k: {nested: 1}}"), Value::new_map(outer));

        // Map with Null value — reuses the document case at the Map level.
        let mut nmap = std::collections::HashMap::new();
        nmap.insert("key".to_string(), Value::Null);
        assert_eq!(text_as_value("{key: null}"), Value::new_map(nmap));

        // Str fallback — `foo: bar` is NOT a top-level map (no `{...}` wrap),
        // so the parser falls through to `Value::Str`.
        assert_eq!(
            text_as_value("foo: bar"),
            Value::Str("foo: bar".to_string())
        );

        // Map with Error value — `{k: error(oops)}`. Top-level `:` lives
        // before the `(`-delimited error contents.
        let mut emap = std::collections::HashMap::new();
        emap.insert("k".to_string(), Value::Error("oops".to_string()));
        assert_eq!(text_as_value("{k: error(oops)}"), Value::new_map(emap));

        // Empty array / empty map wrappers.
        assert_eq!(text_as_value("[]"), Value::new_array(vec![]));
        assert_eq!(
            text_as_value("{}"),
            Value::new_map(std::collections::HashMap::new())
        );

        // ── Error trailing-`)` strip boundary cases (direct hits on
        // the canonical `Error` branch in `parse_value` — no Map wrap
        // so the Error branch fires unambiguously). Locks the
        // `s[6..s.len() - 1]` slice: peels the literal trailing `)`,
        // which on nested-paren messages leaves the inner parens
        // intact. These mirror the boundary fixtures that the deleted
        // `crush-vm::tests::deserialize_recognises_tagged_forms_by_prefix`
        // used to lock (`deserialize_recognises_tagged_forms_by_prefix`
        // covered `vm.rs::visit_str`'s analogous Error branch via the
        // serde path; restored here so the canonical parser-side
        // contract remains explicitly locked).
        assert_eq!(
            text_as_value("error(foo)"),
            Value::Error("foo".to_string())
        );
        assert_eq!(
            text_as_value("error((foo)"),
            Value::Error("(foo".to_string()) // s[6..10] = "(foo"
        );
        assert_eq!(
            text_as_value("error(foo))"),
            Value::Error("foo)".to_string()) // s[6..11] = "foo)"
        );
    }
}
