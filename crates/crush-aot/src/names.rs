//! Crush function names as Rust/C identifiers, shared by both AOT backends.

/// An identifier body for a Crush name. ASCII letters and digits are kept, `_`
/// becomes `__`, and any other character becomes `_u<hex>_`. The mapping is
/// injective (after a `_` comes either `_` or `u`), so two Crush functions never
/// share an identifier: `café` and `cafè`, or `π` and `_`, used to collapse into
/// one name and fail with a duplicate definition. Each backend adds its own prefix.
pub fn mangle(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if c == '_' {
            out.push_str("__");
        } else {
            out.push_str(&format!("_u{:x}_", c as u32));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::mangle;

    #[test]
    fn distinct_names_stay_distinct() {
        let names = ["café", "cafè", "caf_", "caf__", "π", "_", "__", "a_u3c0_", "aπ", "main", "fn_main"];
        let mangled: std::collections::HashSet<_> = names.iter().map(|n| mangle(n)).collect();
        assert_eq!(mangled.len(), names.len(), "{:?}", names.map(mangle));
    }

    #[test]
    fn ascii_names_are_readable() {
        assert_eq!(mangle("main"), "main");
        assert_eq!(mangle("step_row"), "step__row");
        assert_eq!(mangle("π"), "_u3c0_");
    }
}
