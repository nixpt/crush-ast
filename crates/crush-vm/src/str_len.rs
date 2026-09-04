//! Shared string-length semantics used by every `len()` backend.
//!
//! CRUSH-114: `len()` on a string diverged — the CVM1 scheduler and
//! PortableVM errored (`expected array, got str`) while FastVM, the JIT,
//! and both AOT backends returned a length. The canonical semantics is
//! **byte length**, matching the documented `str.len` capability
//! ("byte length of a string") and every backend that already accepted
//! strings (`s.len()` / `strlen`). Every `len`-on-string site calls into
//! [`str_len`] so the backends cannot diverge again (same pattern as
//! `io_print` / `io_read`).
//!
//! Note: `ARR_GET` string indexing is intentionally char-based
//! (`s.chars().nth(...)`) — indexing a character is not the same operation
//! as measuring a string, and this module does not change that.

/// Byte length of a string: the single implementation behind `len(s)` and
/// the `str.len` capability on every backend.
pub fn str_len(s: &str) -> i64 {
    s.len() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_length() {
        assert_eq!(str_len("abc"), 3);
        assert_eq!(str_len(""), 0);
    }

    #[test]
    fn multibyte_is_byte_length_matching_str_len_and_aot() {
        // "héllo": 5 chars, 6 bytes (é is 2 bytes in UTF-8).
        // Pinned to bytes so a future chars().count() "fix" cannot silently
        // diverge from str.len / FastVM / JIT / AOT again.
        assert_eq!(str_len("héllo"), 6);
    }
}
