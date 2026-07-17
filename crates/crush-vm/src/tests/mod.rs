use crate::assembler::{assemble, disassemble};
use crate::vm::{Quotas, Value, run};

/// Inlined canonical Crush-text → Value parser for the matrix test.
/// Mirrors `crush_lang_sdk::caps::text_as_value`: every input falls
/// into one of the recognised forms; unrecognised content falls
/// through to `Value::Str(s)`. **Inlined here, not imported** — a
/// `crush-lang-sdk` dev-dep on `crush-vm` would resolve through a
/// Cargo workspace cycle that links two distinct `crush_vm` instances
/// into the test binary (so `crush_lang_sdk::Value ≠ crush_vm::vm::Value`),
/// breaking the comparison. If the canonical parser in caps.rs ever
/// drifts, this inlined copy must be updated in lockstep. Depth-cap
/// from the original is omitted here because the matrix only feeds
/// well-formed Display output.
// Returns `Value` (not `Option<Value>`), matching the canonical
// `crush_lang_sdk::caps::text_as_value`.
fn parse_crush_text(s: &str) -> Value {
    if s == "null" {
        return Value::Null;
    }
    if s == "true" {
        return Value::Bool(true);
    }
    if s == "false" {
        return Value::Bool(false);
    }
    // Int: must precede Float — e.g. "3.0" fails i64::parse, so this
    // branch is safe; negative integers parse here cleanly.
    if let Ok(i) = s.parse::<i64>() {
        return Value::Int(i);
    }
    // Float: locks the `Display::{:.1}` form (e.g. "3.0" → 3.0_f64).
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
        let parsed = split_top_level_inline(inner, ',')
            .into_iter()
            .map(|p| parse_crush_text(p.trim()))
            .collect();
        return Value::new_array(parsed);
    }

    // Value::Map inverse — `{k: v, k2: v2}` (colon-space, comma-space joined).
    // Mirrors canonical `text_as_value::parse_value` exactly, including
    // the malformed-entry panic-to-Str contract: any pair without a
    // top-level `:` degenerates to `Value::Str(s)` (the whole input, not
    // inner/pair) so the parser cannot accidentally reconstruct a partial
    // map.
    if s_trim.starts_with('{') && s_trim.ends_with('}') {
        let inner = s_trim[1..s_trim.len() - 1].trim();
        if inner.is_empty() {
            return Value::new_map(std::collections::HashMap::new());
        }
        let mut m = std::collections::HashMap::new();
        for pair in split_top_level_inline(inner, ',') {
            if let Some((k, v)) = split_first_top_level_inline(pair.trim(), ':') {
                m.insert(k.trim().to_string(), parse_crush_text(v.trim()));
            } else {
                // Mirror canonical: malformed entry → identity Str(s).
                return Value::Str(s.to_string());
            }
        }
        return Value::new_map(m);
    }

    // Tagged-prefix forms (matches `text_as_value` precedence:
    // `error(msg)` first, then `<N bytes>`, then `<handle N>`,
    // finally Str fallback).
    if s.starts_with("error(") && s.ends_with(')') {
        return Value::Error(s[6..s.len() - 1].to_string());
    }
    if s.starts_with('<') && s.ends_with(" bytes>") {
        if let Ok(n) = s[1..s.len() - 7].parse::<usize>() {
            return Value::Bytes(vec![0; n]);
        }
    }
    if s.starts_with("<handle ") && s.ends_with('>') {
        if let Ok(id) = s[8..s.len() - 1].parse::<u64>() {
            return Value::Handle(id);
        }
    }
    if s.starts_with("<foreign ") && s.ends_with('>') {
        if let Ok(id) = s[9..s.len() - 1].parse::<u64>() {
            return Value::Foreign(id);
        }
    }

    Value::Str(s.to_string())
}

/// Top-level-aware comma separator (matches `text_as_value::split_top_level`).
fn split_top_level_inline(s: &str, delim: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut bd: i32 = 0;
    let mut brd: i32 = 0;
    let mut pd: i32 = 0;
    let mut start = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '[' => bd += 1,
            ']' => bd -= 1,
            '{' => brd += 1,
            '}' => brd -= 1,
            '(' => pd += 1,
            ')' => pd -= 1,
            _ if c == delim && bd == 0 && brd == 0 && pd == 0 => {
                parts.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

/// Top-level-aware first-occurrence separator (matches
/// `text_as_value::split_first_top_level`). Used to peel Map entry
/// `(key, value)` halves at the first top-level `:`.
fn split_first_top_level_inline(s: &str, delim: char) -> Option<(&str, &str)> {
    let mut bd: i32 = 0;
    let mut brd: i32 = 0;
    let mut pd: i32 = 0;
    for (i, c) in s.char_indices() {
        match c {
            '[' => bd += 1,
            ']' => bd -= 1,
            '{' => brd += 1,
            '}' => brd -= 1,
            '(' => pd += 1,
            ')' => pd -= 1,
            _ if c == delim && bd == 0 && brd == 0 && pd == 0 => {
                return Some((&s[..i], &s[i + c.len_utf8()..]));
            }
            _ => {}
        }
    }
    None
}

fn run_src(src: &str) -> crate::vm::VmResult {
    let prog = assemble(src, None, None).expect("assembly");
    run(&prog, &Quotas::default()).expect("vm run")
}

fn run_src_with_perms(src: &str, perms: &[&str]) -> crate::vm::VmResult {
    let prog = assemble(src, Some(perms), None).expect("assembly");
    run(&prog, &Quotas::default()).expect("vm run")
}


// ---- Sub-module declarations ----

#[cfg(test)]
mod arith;

#[cfg(test)]
mod control_flow;

#[cfg(test)]
mod data_types;

#[cfg(test)]
mod capabilities;

#[cfg(test)]
mod surfaces;

#[cfg(test)]
mod async_green;

#[cfg(test)]
mod matrix;

