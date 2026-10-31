use crate::memory::{Arena, Object};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
    Ref(usize), // Index into Arena
    String(String),
}

/// Zero-copy view of numeric values for optimized operations
///
/// This struct provides references to numeric data without cloning,
/// enabling high-performance arithmetic operations.
#[derive(Debug, Copy, Clone)]
pub enum NumericValue<'a> {
    Int(&'a i64),
    Float(&'a f64),
}

/// Zero-copy string view for optimized string operations
///
/// Provides string access without cloning using Cow for optimal memory usage.
#[derive(Debug)]
pub enum StringValue<'a> {
    Borrowed(&'a str),
    Owned(String),
    Cow(Cow<'a, str>),
}

impl<'a> StringValue<'a> {
    /// Create a zero-copy string view from various sources
    #[inline]
    pub fn from_runtime(val: &'a RuntimeValue) -> Option<Self> {
        match val {
            RuntimeValue::String(s) => Some(StringValue::Borrowed(s.as_str())),
            RuntimeValue::Ref(_idx) => {
                // Would need arena access - return None for now
                None
            }
            _ => None,
        }
    }

    /// Get string slice without allocation
    #[inline]
    pub fn as_str(&self) -> &str {
        match self {
            StringValue::Borrowed(s) => s,
            StringValue::Owned(s) => s.as_str(),
            StringValue::Cow(cow) => cow.as_ref(),
        }
    }

    /// Convert to owned String if needed (allocates only when necessary)
    #[inline]
    pub fn into_owned(self) -> String {
        match self {
            StringValue::Borrowed(s) => s.to_string(),
            StringValue::Owned(s) => s,
            StringValue::Cow(cow) => cow.into_owned(),
        }
    }
}

impl<'a> NumericValue<'a> {
    /// Extract numeric value from RuntimeValue without cloning
    #[inline]
    pub fn from_runtime(val: &'a RuntimeValue) -> Option<Self> {
        match val {
            RuntimeValue::Int(i) => Some(NumericValue::Int(i)),
            RuntimeValue::Float(f) => Some(NumericValue::Float(f)),
            _ => None,
        }
    }

    /// Get as i64 if possible (zero conversion for Int)
    #[inline]
    pub fn as_i64(self) -> Option<i64> {
        match self {
            NumericValue::Int(i) => Some(*i),
            NumericValue::Float(f) => Some(*f as i64),
        }
    }

    /// Get as f64 if possible (zero conversion for Float)
    #[inline]
    pub fn as_f64(self) -> Option<f64> {
        match self {
            NumericValue::Int(i) => Some(*i as f64),
            NumericValue::Float(f) => Some(*f),
        }
    }

    /// Perform zero-copy addition
    #[inline]
    pub fn add(self, other: NumericValue) -> RuntimeValue {
        match (self, other) {
            (NumericValue::Int(a), NumericValue::Int(b)) => RuntimeValue::Int(*a + *b),
            (NumericValue::Float(a), NumericValue::Float(b)) => RuntimeValue::Float(*a + *b),
            (NumericValue::Int(a), NumericValue::Float(b)) => RuntimeValue::Float(*a as f64 + *b),
            (NumericValue::Float(a), NumericValue::Int(b)) => RuntimeValue::Float(*a + *b as f64),
        }
    }

    /// Perform zero-copy subtraction
    #[inline]
    pub fn sub(self, other: NumericValue) -> RuntimeValue {
        match (self, other) {
            (NumericValue::Int(a), NumericValue::Int(b)) => RuntimeValue::Int(*a - *b),
            (NumericValue::Float(a), NumericValue::Float(b)) => RuntimeValue::Float(*a - *b),
            (NumericValue::Int(a), NumericValue::Float(b)) => RuntimeValue::Float(*a as f64 - *b),
            (NumericValue::Float(a), NumericValue::Int(b)) => RuntimeValue::Float(*a - *b as f64),
        }
    }

    /// Perform zero-copy multiplication
    #[inline]
    pub fn mul(self, other: NumericValue) -> RuntimeValue {
        match (self, other) {
            (NumericValue::Int(a), NumericValue::Int(b)) => RuntimeValue::Int(*a * *b),
            (NumericValue::Float(a), NumericValue::Float(b)) => RuntimeValue::Float(*a * *b),
            (NumericValue::Int(a), NumericValue::Float(b)) => RuntimeValue::Float(*a as f64 * *b),
            (NumericValue::Float(a), NumericValue::Int(b)) => RuntimeValue::Float(*a * *b as f64),
        }
    }

    /// Perform zero-copy division
    #[inline]
    pub fn div(self, other: NumericValue) -> Result<RuntimeValue, &'static str> {
        match (self, other) {
            (NumericValue::Int(a), NumericValue::Int(b)) => {
                if *b == 0 {
                    Err("Division by zero")
                } else {
                    Ok(RuntimeValue::Int(*a / *b))
                }
            }
            (NumericValue::Float(a), NumericValue::Float(b)) => {
                if *b == 0.0 {
                    Err("Division by zero")
                } else {
                    Ok(RuntimeValue::Float(*a / *b))
                }
            }
            (NumericValue::Int(a), NumericValue::Float(b)) => {
                if *b == 0.0 {
                    Err("Division by zero")
                } else {
                    Ok(RuntimeValue::Float(*a as f64 / *b))
                }
            }
            (NumericValue::Float(a), NumericValue::Int(b)) => {
                if *b == 0 {
                    Err("Division by zero")
                } else {
                    Ok(RuntimeValue::Float(*a / *b as f64))
                }
            }
        }
    }

    /// Perform zero-copy comparison
    #[inline]
    pub fn compare(self, other: NumericValue) -> std::cmp::Ordering {
        match (self, other) {
            (NumericValue::Int(a), NumericValue::Int(b)) => a.cmp(b),
            (NumericValue::Float(a), NumericValue::Float(b)) => {
                a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
            }
            (NumericValue::Int(a), NumericValue::Float(b)) => (*a as f64)
                .partial_cmp(b)
                .unwrap_or(std::cmp::Ordering::Equal),
            (NumericValue::Float(a), NumericValue::Int(b)) => a
                .partial_cmp(&(*b as f64))
                .unwrap_or(std::cmp::Ordering::Equal),
        }
    }
}

impl Eq for RuntimeValue {}

impl Hash for RuntimeValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            RuntimeValue::Int(i) => i.hash(state),
            RuntimeValue::Float(f) => f.to_bits().hash(state),
            RuntimeValue::Bool(b) => b.hash(state),
            RuntimeValue::Null => {}
            RuntimeValue::Ref(r) => r.hash(state),
            RuntimeValue::String(s) => s.hash(state),
        }
    }
}

impl RuntimeValue {
    pub fn as_int(&self) -> Option<i64> {
        if let RuntimeValue::Int(i) = self {
            Some(*i)
        } else {
            None
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        if let RuntimeValue::Bool(b) = self {
            Some(*b)
        } else {
            None
        }
    }

    pub fn as_ref(&self) -> Option<usize> {
        if let RuntimeValue::Ref(idx) = self {
            Some(*idx)
        } else {
            None
        }
    }
}

// Display for RuntimeValue now only prints the immediate value.
// For deep printing, we need the Arena.
impl std::fmt::Display for RuntimeValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeValue::Int(i) => write!(f, "{}", i),
            RuntimeValue::Float(n) => write!(f, "{}", n),
            RuntimeValue::Bool(b) => write!(f, "{}", b),
            RuntimeValue::Null => write!(f, "null"),
            RuntimeValue::Ref(idx) => write!(f, "@{}", idx),
            RuntimeValue::String(s) => write!(f, "\"{}\"", s),
        }
    }
}

// From impls removed as they require Arena for complex types.

/// Canonical structural equality for the `RuntimeValue` + `Arena`
/// representation shared by FastVM and the JIT.
///
/// This mirrors `Value`'s `PartialEq` (the interpreter reference,
/// `crate::vm::Value`) so `EQ`/`NE` cannot drift across engines (CRUSH-215):
///
/// * `Int`/`Float` compare numerically across types (`2 == 2.0`); `Bool` is
///   never coerced to a number, so `true == 1` is `false`.
/// * Strings compare by value whether stored inline (`RuntimeValue::String`)
///   or in the arena (`Object::Str`).
/// * Arrays, tuples, lists, vectors and sets compare element-wise; maps
///   compare by key/value regardless of insertion order.
/// * Struct/object values (`Object::Object`) compare by their fields only —
///   the class name is ignored — and are equal to a `Map` with the same
///   fields, matching the interpreter where struct instances are maps.
/// * Everything else is unequal across kinds.
///
/// FastVM and the JIT both route `EQ`/`NE` through this one function.
pub fn runtime_value_eq(a: &RuntimeValue, b: &RuntimeValue, arena: &Arena) -> bool {
    match (a, b) {
        (RuntimeValue::Null, RuntimeValue::Null) => true,
        (RuntimeValue::Bool(x), RuntimeValue::Bool(y)) => x == y,
        (RuntimeValue::Int(x), RuntimeValue::Int(y)) => x == y,
        (RuntimeValue::Float(x), RuntimeValue::Float(y)) => x == y,
        (RuntimeValue::Int(x), RuntimeValue::Float(y))
        | (RuntimeValue::Float(y), RuntimeValue::Int(x)) => *x as f64 == *y,
        _ => {
            // Strings may be inline or arena-allocated; compare by value
            // before falling through to the object-kind comparison.
            if let (Some(x), Some(y)) = (runtime_str(a, arena), runtime_str(b, arena)) {
                return x == y;
            }
            match (a, b) {
                (RuntimeValue::Ref(x), RuntimeValue::Ref(y)) => {
                    object_eq(arena.get(*x), arena.get(*y), arena)
                }
                _ => false,
            }
        }
    }
}

/// The string a value holds, inline or in the arena.
fn runtime_str<'a>(v: &'a RuntimeValue, arena: &'a Arena) -> Option<&'a str> {
    match v {
        RuntimeValue::String(s) => Some(s.as_str()),
        RuntimeValue::Ref(idx) => match arena.get(*idx) {
            Some(Object::Str(s)) => Some(s.as_str()),
            _ => None,
        },
        _ => None,
    }
}

/// Structural equality of two arena objects (a missing/dead slot is never
/// equal to anything, including another missing slot).
fn object_eq(a: Option<&Object>, b: Option<&Object>, arena: &Arena) -> bool {
    let (Some(a), Some(b)) = (a, b) else {
        return false;
    };
    match (a, b) {
        (Object::Str(x), Object::Str(y)) => x == y,
        (Object::Array(x), Object::Array(y))
        | (Object::Tuple(x), Object::Tuple(y))
        | (Object::Vector(x), Object::Vector(y))
        | (Object::Set(x), Object::Set(y)) => seq_eq(x, y, arena),
        (Object::List(x), Object::List(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y.iter())
                    .all(|(p, q)| runtime_value_eq(p, q, arena))
        }
        (Object::Map(x), Object::Map(y)) => map_eq(x, y, arena),
        (Object::Object { fields: x, .. }, Object::Object { fields: y, .. }) => map_eq(x, y, arena),
        (Object::Object { fields, .. }, Object::Map(y))
        | (Object::Map(y), Object::Object { fields, .. }) => map_eq(fields, y, arena),
        (Object::Bytes(x), Object::Bytes(y)) | (Object::Buffer(x), Object::Buffer(y)) => x == y,
        (Object::Handle(x), Object::Handle(y)) => x == y,
        (Object::InterfaceHandle(x), Object::InterfaceHandle(y)) => x == y,
        (Object::Result { ok: ox, value: x }, Object::Result { ok: oy, value: y }) => {
            ox == oy && runtime_value_eq(x, y, arena)
        }
        (Object::Tagged { tag: tx, value: x }, Object::Tagged { tag: ty, value: y }) => {
            tx == ty && runtime_value_eq(x, y, arena)
        }
        _ => false,
    }
}

fn seq_eq(x: &[RuntimeValue], y: &[RuntimeValue], arena: &Arena) -> bool {
    x.len() == y.len()
        && x.iter()
            .zip(y.iter())
            .all(|(p, q)| runtime_value_eq(p, q, arena))
}

fn map_eq(
    x: &std::collections::HashMap<String, RuntimeValue>,
    y: &std::collections::HashMap<String, RuntimeValue>,
    arena: &Arena,
) -> bool {
    x.len() == y.len()
        && x.iter().all(|(k, v)| match y.get(k) {
            Some(w) => runtime_value_eq(v, w, arena),
            None => false,
        })
}

#[cfg(test)]
mod eq_tests {
    use super::*;
    use crate::memory::{Arena, Object};
    use std::collections::HashMap;

    fn arena_str(arena: &mut Arena, s: &str) -> RuntimeValue {
        RuntimeValue::Ref(arena.alloc(Object::Str(s.to_string())))
    }

    #[test]
    fn scalars_match_the_interpreter() {
        let a = Arena::new();
        use RuntimeValue::*;
        assert!(runtime_value_eq(&Null, &Null, &a));
        assert!(runtime_value_eq(&Bool(true), &Bool(true), &a));
        assert!(!runtime_value_eq(&Bool(true), &Bool(false), &a));
        // Bool is never coerced to a number.
        assert!(!runtime_value_eq(&Bool(true), &Int(1), &a));
        // Int/Float compare numerically across types; NaN is never equal.
        assert!(runtime_value_eq(&Int(2), &Float(2.0), &a));
        assert!(!runtime_value_eq(&Int(2), &Float(2.5), &a));
        assert!(!runtime_value_eq(&Float(f64::NAN), &Float(f64::NAN), &a));
        // Different kinds are unequal.
        assert!(!runtime_value_eq(&Null, &Int(0), &a));
        assert!(!runtime_value_eq(&Int(1), &Null, &a));
    }

    #[test]
    fn strings_compare_by_value_inline_and_in_arena() {
        let mut a = Arena::new();
        let inline = RuntimeValue::String("ab".into());
        let ab1 = arena_str(&mut a, "ab");
        let ab2 = arena_str(&mut a, "ab");
        let ba = arena_str(&mut a, "ba");
        assert!(runtime_value_eq(&inline, &ab1, &a));
        assert!(runtime_value_eq(&ab1, &inline, &a));
        assert!(runtime_value_eq(&ab1, &ab2, &a));
        assert!(!runtime_value_eq(&ab1, &ba, &a));
        assert!(runtime_value_eq(
            &inline,
            &RuntimeValue::String("ab".into()),
            &a
        ));
    }

    #[test]
    fn arrays_and_maps_compare_by_value() {
        let mut a = Arena::new();
        let arr = |a: &mut Arena, xs: &[i64]| {
            RuntimeValue::Ref(a.alloc(Object::Array(
                xs.iter().map(|i| RuntimeValue::Int(*i)).collect(),
            )))
        };
        let a1 = arr(&mut a, &[1, 2, 3]);
        let a2 = arr(&mut a, &[1, 2, 3]);
        let a3 = arr(&mut a, &[1, 3]);
        assert!(runtime_value_eq(&a1, &a2, &a));
        assert!(!runtime_value_eq(&a1, &a3, &a));

        let map = |a: &mut Arena, kvs: &[(&str, i64)]| {
            RuntimeValue::Ref(
                a.alloc(Object::Map(
                    kvs.iter()
                        .map(|(k, v)| (k.to_string(), RuntimeValue::Int(*v)))
                        .collect::<HashMap<_, _>>(),
                )),
            )
        };
        let m1 = map(&mut a, &[("a", 1), ("b", 2)]);
        let m2 = map(&mut a, &[("b", 2), ("a", 1)]); // order-insensitive
        let m3 = map(&mut a, &[("a", 1), ("b", 3)]);
        assert!(runtime_value_eq(&m1, &m2, &a));
        assert!(!runtime_value_eq(&m1, &m3, &a));
        // Array and map are different kinds.
        assert!(!runtime_value_eq(&a1, &m1, &a));
    }

    #[test]
    fn structs_compare_by_fields_ignoring_class_name() {
        let mut a = Arena::new();
        let object = |a: &mut Arena, class: &str, x: i64| {
            RuntimeValue::Ref(a.alloc(Object::Object {
                lang: "crush".into(),
                class_name: class.into(),
                fields: HashMap::from([("x".to_string(), RuntimeValue::Int(x))]),
            }))
        };
        let p = object(&mut a, "P", 1);
        let q = object(&mut a, "Q", 1);
        let p2 = object(&mut a, "P", 2);
        assert!(
            runtime_value_eq(&p, &q, &a),
            "structs equal by fields, not name"
        );
        assert!(!runtime_value_eq(&p, &p2, &a));
        // A struct equals a map with the same fields (struct instances are maps).
        let m = RuntimeValue::Ref(a.alloc(Object::Map(HashMap::from([(
            "x".to_string(),
            RuntimeValue::Int(1),
        )]))));
        assert!(runtime_value_eq(&p, &m, &a));
        assert!(runtime_value_eq(&m, &p, &a));
    }
}
