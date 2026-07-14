//! Tests for the combined round-trip + cross-parser matrix domain.
//!
//! Auto-extracted from `tests.rs` as part of CRUSHTESTSSPLIT-1 (v2).
//!
//! Each fn preserves its original body verbatim; only the
//! section-banner organizer moved into a sub-file. Multi-line
//! banners are merged into a single classification.

use super::*;
use crate::assembler::{assemble, disassemble};
use crate::vm::{Quotas, Value, run};

// ── Combined round-trip matrix (single source-of-truth for the canonical ────
// ── trait triplet: Display / Serialize / Deserialize / text_as_value)     ────

#[test]
fn all_traits_round_trip_for_every_variant() {
    // Single source-of-truth matrix for the canonical trait triplet on
    // `crush_vm::vm::Value`:
    //
    //   `impl Display`           (line-rendering — canonical Crush text)
    //   `impl serde::Serialize`  (JSON wire-format)
    //   `impl serde::Deserialize` (canonical inverse of Serialize,
    //     including the tagged-form `<handle N>` / `<N bytes>` /
    //     `error(msg)` precedence in `visit_str`)
    //   `text_as_value`           (canonical Crush-text → Value parser;
    //     lives in `crush-lang-sdk::caps`, exercised here via the
    //     `crush-lang-sdk` dev-dep added to `crush-vm/Cargo.toml`).
    //
    // For each variant, four invariants are asserted under one matrix:
    //
    //   1. Display output is the canonical Crush text form —
    //      non-empty for every non-Null variant; explicit `"null"`
    //      for Null. (Sanity — no regressions that emit `""` for a
    //      Null, no regressions that emit `"(true)"` for a Bool, etc.)
    //
    //   2. `text_as_value ∘ Display == id` (i.e., the canonical Crush
    //      text form parses back to v).
    //
    //   3. `Deserialize ∘ Serialize == id` (i.e., the canonical JSON
    //      wire-format parses back to v).
    //
    //   4. For tagged forms (`Error`, `Bytes`, `Handle`), the
    //      inner-text segment produced by Serialize (the substring
    //      within the JSON string literal) equals what Display
    //      produces for the same variant — confirming the
    //      `<handle N>` / `<N bytes>` / `error(msg)` lockstep across
    //      the two formatters.
    //
    // Replaces the following 6 redundant tests with one matrix:
    //   - `display_map_renders_null_and_float_canonically`
    //   - `display_empty_map_renders_as_two_braces`
    //   - `serialize_produces_canonical_json_for_every_variant`
    //   - `deserialize_is_serialize_inverse_for_every_variant`
    //   - `deserialize_recognises_tagged_forms_by_prefix`
    //   - `text_as_value_is_display_inverse_for_every_variant` (caps.rs)
    //
    // Tests NOT removed by this matrix (kept because they exercise
    // distinctive concerns not covered here):
    //   - `caps::tests::text_as_value_edge_cases` — bracket-only inputs
    //     (`[[1,2],[3]]`, `{k:{nested:1}}`, `{key:null}`, `{k:error(oops)}`)
    //     that go through `text_as_value` path directly (no Display
    //     round-trip), exercising top-level-aware `split_*` helpers
    //     and the canonical Error-tagged nesting
    //   - `test_conv_to_str` / `test_json_parse` / `test_json_stringify`
    //     etc. (cap-gated integration tests in stdlib.rs + caps.rs)
    //   - the pre-fix `arr.len()` → `arr.borrow().len()` breadcrumb
    //     locks (one-liner structural regressions)

    let mut nested_inner = std::collections::HashMap::<String, Value>::new();
    nested_inner.insert("nested".to_string(), Value::Int(1));
    let mut nested_outer = std::collections::HashMap::<String, Value>::new();
    nested_outer.insert("outer".to_string(), Value::new_map(nested_inner));
    nested_outer.insert("sibling".to_string(), Value::Bool(true));

    let variants: Vec<(&str, Value)> = vec![
        ("Null", Value::Null),
        ("Bool true", Value::Bool(true)),
        ("Bool false", Value::Bool(false)),
        ("Int -7", Value::Int(-7)),
        ("Int 0 (sign-edge)", Value::Int(0)),
        ("Float 3.14 (fractional)", Value::Float(3.14)),
        ("Float 0.0 (zero-edge)", Value::Float(0.0)),
        ("Float 3.0 (.0 suffix lockstep)", Value::Float(3.0)),
        ("Str foo", Value::Str("foo".to_string())),
        ("Str with escapes", Value::Str(r#"a"b\c"#.to_string())),
        (
            "Bytes 3 (length-only caveat, zero-fill)",
            Value::Bytes(vec![0, 0, 0]),
        ),
        ("Handle 42 (tagged-form lockstep)", Value::Handle(42)),
        ("Foreign 42 (tagged-form lockstep)", Value::Foreign(42)),
        ("Error oops (tagged-form lockstep)", Value::Error("oops".to_string())),
        (
            "Array [1, 2] (single-level)",
            Value::new_array(vec![Value::Int(1), Value::Int(2)]),
        ),
        (
            "Array nested (locks visit_seq recursion)",
            Value::new_array(vec![
                Value::new_array(vec![Value::Int(1), Value::Int(2)]),
                Value::new_array(vec![Value::Int(3)]),
            ]),
        ),
        ("Array empty (edge case)", Value::new_array(vec![])),
        (
            "Map {k:42, k2:\"v\"} (single-level)",
            Value::new_map({
                let mut m = std::collections::HashMap::<String, Value>::new();
                m.insert("k".to_string(), Value::Int(42));
                m.insert("k2".to_string(), Value::Str("v".to_string()));
                m
            }),
        ),
        ("Map nested (locks visit_map recursion)", Value::new_map(nested_outer)),
        ("Map empty (edge case)", Value::new_map(std::collections::HashMap::<String, Value>::new())),
    ];

    for (label, v) in variants {
        // Invariant 1: Display produces canonical Crush text form.
        let display_str = v.to_string();
        match &v {
            Value::Null => assert_eq!(
                display_str, "null",
                "{label}: Display should be 'null' for Value::Null, got {display_str:?}"
            ),
            _ => assert!(
                !display_str.is_empty(),
                "{label}: Display output should be non-empty, got {display_str:?}"
            ),
        }

        // Invariant 2: text_as_value ∘ Display == identity.
        let parsed_via_display = parse_crush_text(&display_str);
        assert_eq!(
            parsed_via_display, v,
            "{label}: text_as_value(Display(v)) != v; Display was {display_str:?}, parsed={parsed_via_display:?}"
        );

        // Invariant 3: Deserialize ∘ Serialize == identity.
        let json_str = serde_json::to_string(&v)
            .unwrap_or_else(|e| panic!("{label}: Serialize failed: {e}"));
        let parsed_via_json: Value = serde_json::from_str(&json_str)
            .unwrap_or_else(|e| panic!("{label}: Deserialize failed for {json_str:?}: {e}"));
        assert_eq!(
            parsed_via_json, v,
            "{label}: Deserialize(Serialize(v)) != v; Serialize was {json_str:?}, parsed={parsed_via_json:?}"
        );

        // Invariant 4: For tagged forms, lockstep between Display text
        // form and the inner-text segment of Serialize's JSON output.
        // The Serialize-text inner segment is the JSON-string body of
        // the JSON-quoted form; e.g. Display emits "<handle 42>" and
        // Serialize emits "\"<handle 42>\"" (with surrounding JSON
        // quotes), so the inner segment is identical after stripping.
        match &v {
            Value::Error(e) => {
                let expected_inner = format!("error({e})");
                assert_eq!(
                    display_str, expected_inner,
                    "{label}: Display text should be {expected_inner:?}, got {display_str:?}"
                );
                let expected_json = format!("\"{expected_inner}\"");
                assert_eq!(
                    json_str, expected_json,
                    "{label}: Serialize JSON should be {expected_json:?}, got {json_str:?}"
                );
            }
            Value::Bytes(b) => {
                let expected_inner = format!("<{} bytes>", b.len());
                assert_eq!(
                    display_str, expected_inner,
                    "{label}: Display text should be {expected_inner:?}, got {display_str:?}"
                );
                let expected_json = format!("\"{expected_inner}\"");
                assert_eq!(
                    json_str, expected_json,
                    "{label}: Serialize JSON should be {expected_json:?}, got {json_str:?}"
                );
            }
            Value::Handle(id) => {
                let expected_inner = format!("<handle {id}>");
                assert_eq!(
                    display_str, expected_inner,
                    "{label}: Display text should be {expected_inner:?}, got {display_str:?}"
                );
                let expected_json = format!("\"{expected_inner}\"");
                assert_eq!(
                    json_str, expected_json,
                    "{label}: Serialize JSON should be {expected_json:?}, got {json_str:?}"
                );
            }
            Value::Foreign(id) => {
                let expected_inner = format!("<foreign {id}>");
                assert_eq!(
                    display_str, expected_inner,
                    "{label}: Display text should be {expected_inner:?}, got {display_str:?}"
                );
                let expected_json = format!("\"{expected_inner}\"");
                assert_eq!(
                    json_str, expected_json,
                    "{label}: Serialize JSON should be {expected_json:?}, got {json_str:?}"
                );
            }
            _ => {}
        }
    }
}

    #[test]
    fn test_json_parse_bytes_lossy_round_trip_inline() {
        // **Trait-layer lock for the `<N bytes>` lossy round-trip**:
        // The canonical `impl Serialize for Value::Bytes(b)` emits
        // ONLY the length-prefix inner-content `<{N} bytes>` (e.g.
        // `<3 bytes>` for `vec![1,2,3]`); actual byte contents are
        // NOT preserved through the JSON wire. `serde_json::to_string`
        // wraps that inner tag in surrounding JSON quotes before
        // returning the 11-char Rust String `r#""<3 bytes>""#`.
        // Re-parsing the recovered JSON-quoted tag via canonical
        // `impl Deserialize for Value::visit_str` reconstructs a
        // ZERO-FILLED `Vec<u8>` of length N — NOT the original
        // byte payload.
        //
        // This TRAIT-LAYER test pins the lossiness contract
        // end-to-end through the canonical `serde` trait impls
        // (NOT through the `json.parse`/`json.stringify` cap layer,
        // which is locked separately in
        // `crush-lang-sdk::tests::test_json_parse_tagged_forms::
        // fixture 6`). Drift in either trait impl would surface
        // here as an `assert_eq!` mismatch, NOT silently pass
        // through either path layer.

        let bytes_value = Value::Bytes(vec![1u8, 2, 3]);

        // Step A: `serde_json::to_string(&Value::Bytes(vec![1,2,3]))`
        // emits the JSON-quoted length-only tag `r#""<3 bytes>""#` —
        // byte CONTENTS dropped, length preserved. The trait impl
        // emits the bare inner tag `<3 bytes>` (9 chars); serde_json
        // wraps it in surrounding `"`s before returning the 11-char
        // String.
        let serialized_json = serde_json::to_string(&bytes_value)
            .expect("Serialize for Value::Bytes should not fail");
        assert_eq!(
            serialized_json, r#""<3 bytes>""#,
            "canonical Serialize for Value::Bytes(vec![1,2,3]) at the trait layer \
             should emit the JSON-quoted length-only tag \"<3 bytes>\" (byte \
             contents intentionally stripped), got {serialized_json:?}"
        );

        // Step B: `serde_json::from_str::<Value>(&"\"<3 bytes>\"")`
        // reconstructs a ZERO-FILLED Vec<u8> of length N — NOT the
        // original `vec![1, 2, 3]` payload. Documented length-only
        // caveat; byte preservation through JSON wire format is
        // NOT a goal.
        let parsed: Value = serde_json::from_str(&serialized_json)
            .expect("Deserialize for \"<3 bytes>\" should not fail");
        match parsed {
            Value::Bytes(reconstructed) => assert_eq!(
                reconstructed, vec![0u8, 0, 0],
                "LOSSY ROUND-TRIP: canonical Deserialize for \"<3 bytes>\" \
                 reconstructs a ZERO-FILLED Vec<u8> of length N (NOT the \
                 original byte payload vec![1,2,3]). Got {:?}, expected \
                 vec![0,0,0].",
                reconstructed
            ),
            other => panic!(
                "FAIL: canonical Deserialize for \"<3 bytes>\" should produce \
                 Value::Bytes(vec![0,0,0]) (zero-filled per the length-only \
                 caveat), got {other:?}"
            ),
        }        // No Step C: Steps A+B jointly prove `parsed != bytes_value` (Step A
        // pins Serialize's exact `<3 bytes>` form, Step B pins
        // Deserialize's exact `vec![0,0,0]` reconstruction). An
        // `assert_ne!` here would also move-conflict with Step B's
        // `Value::Bytes(reconstructed)` binding.
    }

    // ── cross-parser matrix ────────────────────────────────────────
    //
    // Locks the JSON-text-vs-Crush-text inverse parallelism for the
    // FOUR boundary fixtures that historically are the parser-drift
    // surface. Each fixture is fed through BOTH the Crush-text path
    // (the inlined `parse_crush_text` mirror of canonical
    // `caps::parse_value` — see its docstring for the Cargo-cycle
    // rationale that prevents direct `caps::parse_value` invocation
    // from `crush-vm::tests`) AND the JSON path (canonical
    // `impl Deserialize for Value::visit_str`, exercised via
    // `serde_json::from_str::<Value>(&serde_json::to_string(
    // &serde_json::Value::String(content.to_string()))?)`). The
    // third assertion on each fixture is the cross-parity lock —
    // if ONE side drifts from the other, the panic names the
    // affected fixture and which side produced the divergent value.
    //
    // Companion to `all_traits_round_trip_for_every_variant` (which
    // locks `text_as_value ∘ Display == id` and `Deserialize ∘
    // Serialize == id` separately for every variant). THIS test
    // adds the linkage: text-path output === JSON-path output for
    // the SAME canonical content at the parser-drift boundary.
    // Without this linkage, `caps::parse_value` could drift from
    // `impl Deserialize::visit_str` (or vice-versa) silently — a
    // `json.parse("error((foo)")` could land on one canonical form
    // while `crush -e 'error((foo)'` (text path) lands on another.
    //
    // Drift sources caught:
    //  • `impl Deserialize::visit_str` → `accept ! (1)` fails.
    //  • `caps::parse_value` (via mirror drift) → `accept ! (1)`
    //    fails; reader compared `from_json` to canonical-expected
    //    and panic names JSON-side first (canonical) before text.
    //  • `Value::Display` for the tagged forms → on Display round-
    //    trip, neither path would reach the boundary fixture's
    //    expected output; this also catches that drift.
    #[test]
    fn test_text_vs_json_inverse_parser_matrix() {
        // (canonical_content, expected_value_after_parse)
        //
        // Each entry is parsed via `parse_crush_text(content)`
        // (the inlined mirror of canonical `caps::text_as_value`)
        // AND `serde_json::from_str::<Value>(&serde_json::to_string(
        // &serde_json::Value::String(content.to_string())).unwrap())`
        // — JSON-quoting via the canonical `serde_json::Value::String`
        // pipeline matches the wire-form the cap layer
        // (`json.stringify` → `json.parse`) produces end-to-end.
        let fixtures: &[(&str, Value)] = &[
            // Boundary 1: `<handle N>` tagged form. Both paths
            // extract the integer `N` from inside the brackets.
            ("<handle 42>", Value::Handle(42)),
            ("<foreign 42>", Value::Foreign(42)),
            // Boundary 2: `<N bytes>` length-tag. Both paths
            // reconstruct a zero-filled `Vec<u8>` of length N
            // (documented length-only caveat — actual byte payload
            // is NOT preserved through either path).
            ("<3 bytes>", Value::Bytes(vec![0u8, 0, 0])),
            // Boundary 3: `error((foo)` nested-open. The
            // `s[6..s.len() - 1]` slice formula strips ONE leading
            // wrap and ONE trailing `)`, preserving the
            // inner-most opening paren. NOT a balanced-paren walk.
            (
                "error((foo)",
                Value::Error("(foo".to_string()),
            ),
            // Boundary 4: `error(foo))` nested-close. Same slice,
            // preserves the inner-most closing paren.
            (
                "error(foo))",
                Value::Error("foo)".to_string()),
            ),
        ];

        for &(content, ref expected) in fixtures {
            // Crush-text path: direct canonical content via the
            // inlined mirror of `caps::text_as_value`.
            let from_text = parse_crush_text(content);
            assert_eq!(
                from_text, *expected,
                "TEXT-side drift on fixture {:?}: parse_crush_text({:?}) \
                 produced {:?}, expected {:?}",
                content, content, from_text, *expected
            );

            // JSON path: route through canonical
            // `serde_json::Value::String(content).to_string()` to
            // produce a JSON-quoted envelope, then parse back
            // through canonical `impl Deserialize for Value`. This
            // mirrors the wire-form the cap layer produces.
            let json_quoted = serde_json::to_string(
                &serde_json::Value::String(content.to_string()),
            )
            .expect("serde_json::Value::String always serializes");
            let from_json: Value = serde_json::from_str(&json_quoted)
                .expect("JSON-quoted canonical content always parses");
            assert_eq!(
                from_json, *expected,
                "JSON-side drift on fixture {:?}: \
                 serde_json::from_str::<Value>({}) produced {:?}, \
                 expected {:?}",
                content, json_quoted, from_json, *expected
            );

            // CROSS-PARSER PARITY: text-path output MUST equal
            // JSON-path output for the same canonical content. If
            // ONE side drifts from the other, this assertion
            // pinpoints which side diverged. The panic message
            // also dumps `*expected` so a future debugger can
            // identify the drifter BY INSPECTION: the side
            // (text vs json) that differs from `expected` is the
            // one that drifted. This is the regression lock that
            // the user's audit flagged as missing — without
            // this assertion, `caps::parse_value` and
            // `impl Deserialize::visit_str` could drift
            // independently without CI catching it.
            assert_eq!(
                from_text, from_json,
                "CROSS-PARSER DRIFT on canonical content {:?}: \
                 text-path={:?}, JSON-path={:?}, expected={:?}. \
                 Either `caps::parse_value` (and its test-mirror \
                 `parse_crush_text`) OR `impl Deserialize::visit_str` \
                 drifted from each other; by inspection, the side \
                 that differs from `expected` is the drifter. \
                 Companion matrices: see the doc-comment on this test \
                 function and on `parse_crush_text`.",
                content, from_text, from_json, *expected,
            );
        }
    }
