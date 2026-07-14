use serde::{Deserialize, Serialize};

/// Language-agnostic type system for CAST
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[derive(Default)]
pub enum CastType {
    /// 64-bit integer
    Int,
    /// 64-bit floating point
    Float,
    /// 32-bit floating point
    F32,
    /// Arbitrary-precision integer
    BigInt,
    /// Complex number (f64 real, f64 imag)
    Complex,
    /// N-dimensional tensor/matrix
    Tensor(Box<CastType>),
    /// UTF-8 string
    String,
    /// Boolean
    Bool,
    /// Null/Unit value
    Null,
    /// Homogeneous array
    Array(Box<CastType>),
    /// Key-value map (String keys)
    Map(Box<CastType>),
    /// Named structure or class
    Struct(String),
    /// Function/Lambda with param types and return type
    Lambda {
        params: Vec<CastType>,
        returns: Box<CastType>,
    },
    /// Any/Dynamic type
    #[default]
    Any,
    /// Reference to a defined type
    TypeRef(String),
}

impl std::fmt::Display for CastType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Int => write!(f, "Int"),
            Self::Float => write!(f, "Float"),
            Self::F32 => write!(f, "F32"),
            Self::BigInt => write!(f, "BigInt"),
            Self::Complex => write!(f, "Complex"),
            Self::Tensor(t) => write!(f, "Tensor<{}>", t),
            Self::String => write!(f, "String"),
            Self::Bool => write!(f, "Bool"),
            Self::Null => write!(f, "Null"),
            Self::Array(t) => write!(f, "Array<{}>", t),
            Self::Map(t) => write!(f, "Map<String, {}>", t),
            Self::Struct(s) => write!(f, "Struct<{}>", s),
            Self::Lambda { params, returns } => {
                let params_str: Vec<String> = params.iter().map(|p| p.to_string()).collect();
                write!(f, "fn({}) -> {}", params_str.join(", "), returns)
            }
            Self::Any => write!(f, "Any"),
            Self::TypeRef(s) => write!(f, "{}", s),
        }
    }
}
