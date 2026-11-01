//! The one float-to-text rule (CRUSH-229). Std only: embedded verbatim by Rust AOT.

/// A float the way Crush prints it.
pub fn crush_float_to_text(f: f64) -> String {
    if f.is_finite() && f.fract() == 0.0 {
        format!("{f:.1}")
    } else {
        f.to_string()
    }
}
