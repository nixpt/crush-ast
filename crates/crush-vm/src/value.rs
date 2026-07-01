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
