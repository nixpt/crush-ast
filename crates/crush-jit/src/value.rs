//! Nan-boxed value representation for JIT-compiled Crush code.
//!
//! A single `u64` can represent small `Int`, `Float`, `Bool`, `Null`, or `Ref`.
//!
//! Bit layout (MSB first): [1 sign | 11 exponent | 52 mantissa]
//!
//! Tags use distinct top-4-nibbles of the mantissa for mutual disambiguation:
//!
//!   0x7FFC_XXXX_XXXX_XXXX  → {Null=0x0000, True=0x0001, False=0x0002} + reserved
//!   0x7FFD_XXXX_XXXX_XXXX  → Small Int  (lower 48 bits = sign-extended i16)
//!   0x7FFE_XXXX_XXXX_XXXX  → Heap Ref   (lower 48 bits = arena index)
//!   anything else           → Float (includes canonical NaN 0x7FF8*)

use std::fmt;

pub(crate) const TAG_NULL: u64  = 0x7FFC_0000_0000_0000;
pub(crate) const TAG_TRUE: u64  = 0x7FFC_0000_0000_0001;
pub(crate) const TAG_FALSE: u64 = 0x7FFC_0000_0000_0002;
pub(crate) const TAG_INT: u64   = 0x7FFD_0000_0000_0000;
pub(crate) const TAG_REF: u64   = 0x7FFE_0000_0000_0000;
pub(crate) const MASK_TOP16: u64 = 0xFFFF_0000_0000_0000;
pub(crate) const REF_PAYLOAD: u64 = 0x0000_FFFF_FFFF_FFFF; // lower 48 bits
pub(crate) const MASK_SPECIAL: u64 = 0x7FFC_0000_0000_0000;
pub(crate) const MASK_INT: u64 = 0x7FFD_0000_0000_0000;
pub(crate) const MASK_REF: u64  = 0x7FFE_0000_0000_0000;

/// A 64-bit nan-boxed value that maps to Crush's [`RuntimeValue`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JitValue(pub u64);

impl JitValue {
    #[inline]
    pub fn int(v: i64) -> Self {
        debug_assert!(v >= -0x8000 && v <= 0x7FFF, "large ints not yet supported in JIT Phase 1");
        let bits = (v as u64) & 0xFFFF;
        Self(TAG_INT | bits)
    }

    #[inline]
    pub fn float(v: f64) -> Self {
        Self(v.to_bits())
    }

    #[inline]
    pub fn bool(v: bool) -> Self {
        if v { Self(TAG_TRUE) } else { Self(TAG_FALSE) }
    }

    #[inline]
    pub fn null() -> Self {
        Self(TAG_NULL)
    }

    /// Returns true if this is a tagged value (not a raw float/NaN).
    #[inline]
    pub fn is_tagged(self) -> bool {
        let top16 = self.0 & MASK_TOP16;
        top16 == MASK_SPECIAL || top16 == MASK_INT || top16 == MASK_REF
    }

    #[inline]
    pub fn is_null(self) -> bool {
        self.0 == TAG_NULL
    }

    #[inline]
    pub fn to_bool(self) -> Option<bool> {
        if self.0 == TAG_TRUE { Some(true) }
        else if self.0 == TAG_FALSE { Some(false) }
        else { None }
    }

    /// Check if this is a small int.
    #[inline]
    pub fn is_int(self) -> bool {
        (self.0 & MASK_TOP16) == MASK_INT
    }

    /// Extract the int value.
    #[inline]
    pub fn to_int(self) -> Option<i64> {
        if self.is_int() {
            let low16 = (self.0 & 0xFFFF) as i16 as i64;
            Some(low16)
        } else {
            None
        }
    }

    /// Returns true if this value is a float (any non-tagged pattern, including NaN).
    #[inline]
    pub fn is_float(self) -> bool {
        !self.is_tagged()
    }

    /// Extract the float value.
    #[inline]
    pub fn to_float(self) -> Option<f64> {
        if !self.is_tagged() {
            Some(f64::from_bits(self.0))
        } else {
            None
        }
    }

    #[inline]
    pub fn is_ref(self) -> bool {
        (self.0 & MASK_TOP16) == MASK_REF
    }

    #[inline]
    pub fn to_ref(self) -> Option<usize> {
        if self.is_ref() {
            Some((self.0 & REF_PAYLOAD) as usize)
        } else {
            None
        }
    }

    #[inline]
    pub fn from_ref(idx: usize) -> Self {
        Self(TAG_REF | (idx as u64 & REF_PAYLOAD))
    }

    pub fn type_name(self) -> &'static str {
        if self.is_null() { "null" }
        else if self.0 == TAG_TRUE || self.0 == TAG_FALSE { "bool" }
        else if self.is_int() { "int" }
        else if self.is_float() { "float" }
        else if self.is_ref() { "ref" }
        else { "unknown" }
    }

    #[inline]
    pub fn is_truthy(self) -> bool {
        self.0 != TAG_FALSE && self.0 != TAG_NULL
    }

    pub fn as_bool(self) -> bool {
        self.to_bool().expect("not a bool")
    }

    pub fn as_int(self) -> i64 {
        self.to_int().expect("not an int")
    }

    pub fn as_float(self) -> f64 {
        self.to_float().expect("not a float")
    }
}

impl From<i64> for JitValue {
    fn from(v: i64) -> Self { Self::int(v) }
}

impl From<f64> for JitValue {
    fn from(v: f64) -> Self { Self::float(v) }
}

impl From<bool> for JitValue {
    fn from(v: bool) -> Self { Self::bool(v) }
}

impl fmt::Display for JitValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(v) = self.to_int() { write!(f, "{v}") }
        else if let Some(v) = self.to_float() { write!(f, "{v}") }
        else if self.0 == TAG_TRUE { write!(f, "true") }
        else if self.0 == TAG_FALSE { write!(f, "false") }
        else if self.is_null() { write!(f, "null") }
        else if self.is_ref() { write!(f, "ref#{}", self.to_ref().unwrap()) }
        else { write!(f, "<jit:{:#018x}>", self.0) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_roundtrip() {
        for v in [-32768i64, -1, 0, 1, 42, 127, 32767] {
            let jv = JitValue::int(v);
            assert!(jv.is_int(), "{v} should be int");
            assert_eq!(jv.to_int(), Some(v), "int {v} roundtrip");
        }
    }

    #[test]
    fn int_zero_not_null() {
        let z = JitValue::int(0);
        assert!(z.is_int());
        assert!(!z.is_null());
        assert_eq!(z.to_int(), Some(0));
    }

    #[test]
    fn float_roundtrip() {
        for v in [0.0, -1.0, 3.14, 1e100, f64::MIN, f64::MAX] {
            let jv = JitValue::float(v);
            assert!(jv.is_float(), "{v} should be float");
            assert_eq!(jv.to_float(), Some(v), "float {v} roundtrip");
        }
    }

    #[test]
    fn bool_roundtrip() {
        assert_eq!(JitValue::bool(true).to_bool(), Some(true));
        assert_eq!(JitValue::bool(false).to_bool(), Some(false));
    }

    #[test]
    fn null_value() {
        assert!(JitValue::null().is_null());
        assert!(!JitValue::int(0).is_null());
    }

    #[test]
    fn int_not_float() {
        let v = JitValue::int(42);
        assert!(v.is_int());
        assert!(!v.is_float());
        assert!(v.to_float().is_none());
    }

    #[test]
    fn float_not_int() {
        let v = JitValue::float(3.14);
        assert!(v.is_float());
        assert!(!v.is_int());
        assert!(v.to_int().is_none());
    }

    #[test]
    fn nan_is_float() {
        let v = JitValue::float(f64::NAN);
        assert!(v.is_float(), "canonical NaN should be float");
        assert!(!v.is_ref(), "NaN should not be detected as ref");
        assert!(!v.is_int(), "NaN should not be detected as int");
    }

    #[test]
    fn type_names() {
        assert_eq!(JitValue::int(1).type_name(), "int");
        assert_eq!(JitValue::float(1.0).type_name(), "float");
        assert_eq!(JitValue::bool(true).type_name(), "bool");
        assert_eq!(JitValue::null().type_name(), "null");
        assert_eq!(JitValue::from_ref(42).type_name(), "ref");
    }

    #[test]
    fn truthy_values() {
        assert!(JitValue::int(1).is_truthy());
        assert!(JitValue::int(0).is_truthy());
        assert!(JitValue::float(0.0).is_truthy());
        assert!(JitValue::bool(true).is_truthy());
        assert!(!JitValue::bool(false).is_truthy());
        assert!(!JitValue::null().is_truthy());
    }

    #[test]
    fn ref_roundtrip() {
        let r = JitValue::from_ref(42);
        assert!(r.is_ref());
        assert_eq!(r.to_ref(), Some(42));
        assert_eq!(r.to_int(), None);
        assert!(!r.is_float());
    }

    #[test]
    fn nan_and_ref_are_distinct() {
        let nan = JitValue::float(f64::NAN);
        let r = JitValue::from_ref(0);
        assert!(nan.is_float());
        assert!(r.is_ref());
        assert_ne!(nan.0, r.0);
    }
}
