//! The one float-to-text rule behind `io.print` and string concatenation (CRUSH-229).
//!
//! Shortest round-trip digits, never an exponent, `.0` on finite whole numbers
//! (`-0.0` stays `-0.0`), `NaN` / `inf` / `-inf` as Rust spells them.
//!
//! The rule is in `float_text_core.rs` (std only), which Rust AOT embeds verbatim
//! ([`SOURCE`], same pattern as `rust_debug_core`). The C backend cannot call Rust:
//! its `_float_text` is a hand port, held to this function by the differential test
//! `crush-aot/tests/float_text_agreement.rs`.

pub use crate::float_text_core::crush_float_to_text;


/// The core file's text, for the Rust AOT code generator to embed.
pub const SOURCE: &str = include_str!("float_text_core.rs");

#[cfg(test)]
mod tests {
    use super::crush_float_to_text as t;

    #[test]
    fn the_rule() {
        assert_eq!(t(1.0), "1.0");
        assert_eq!(t(-0.0), "-0.0");
        assert_eq!(t(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(t(100000000000000000000.0), "100000000000000000000.0");
        assert_eq!(t(1.0 / 10_000_000.0), "0.0000001");
        assert_eq!(t(f64::NAN), "NaN");
        assert_eq!(t(f64::INFINITY), "inf");
        assert_eq!(t(f64::NEG_INFINITY), "-inf");
    }
}
