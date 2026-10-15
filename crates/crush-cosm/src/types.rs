//! Field and signature types.
//!
//! [`CosmType`] mirrors `crush_cast::CastType` variant for variant, with the
//! **same JSON shape** (`"Int"`, `{"Array":"Int"}`, `{"Struct":"Point"}`,
//! `{"Lambda":{"params":[…],"returns":…}}`), so a CAST type serializes into a
//! COSM field unchanged.
//!
//! It is a mirror rather than a re-export because `crush-cast` depends
//! (through `crush-caison`) on `crush-vm`, and the VM (CRUSH-zorro-43) and
//! CASM (CRUSH-zorro-42) must be able to depend on this crate without a cycle.
//! A front end that has both crates converts with a serde round-trip or a
//! one-line `match`.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A COSM type: `crush_cast::CastType`'s twin.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CosmType {
    Int,
    Float,
    F32,
    BigInt,
    Complex,
    Tensor(Box<CosmType>),
    String,
    Bool,
    Null,
    Array(Box<CosmType>),
    Tuple(Vec<CosmType>),
    List(Box<CosmType>),
    Vector(Box<CosmType>),
    Set(Box<CosmType>),
    Map(Box<CosmType>),
    /// A named structure or class (a Kind).
    Struct(std::string::String),
    Lambda {
        params: Vec<CosmType>,
        returns: Box<CosmType>,
    },
    /// Dynamic / unannotated.
    #[default]
    Any,
    /// A reference to a defined type.
    TypeRef(std::string::String),
}

impl fmt::Display for CosmType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let list = |ts: &[CosmType]| ts.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(", ");
        match self {
            CosmType::Int => write!(f, "Int"),
            CosmType::Float => write!(f, "Float"),
            CosmType::F32 => write!(f, "F32"),
            CosmType::BigInt => write!(f, "BigInt"),
            CosmType::Complex => write!(f, "Complex"),
            CosmType::Tensor(t) => write!(f, "Tensor<{t}>"),
            CosmType::String => write!(f, "String"),
            CosmType::Bool => write!(f, "Bool"),
            CosmType::Null => write!(f, "Null"),
            CosmType::Array(t) => write!(f, "Array<{t}>"),
            CosmType::Tuple(ts) => write!(f, "Tuple<{}>", list(ts)),
            CosmType::List(t) => write!(f, "List<{t}>"),
            CosmType::Vector(t) => write!(f, "Vector<{t}>"),
            CosmType::Set(t) => write!(f, "Set<{t}>"),
            CosmType::Map(t) => write!(f, "Map<String, {t}>"),
            CosmType::Struct(s) => write!(f, "Struct<{s}>"),
            CosmType::Lambda { params, returns } => write!(f, "fn({}) -> {returns}", list(params)),
            CosmType::Any => write!(f, "Any"),
            CosmType::TypeRef(s) => write!(f, "{s}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CosmType;
    use serde_json::json;

    /// The JSON spellings `crush_cast::CastType` produces (externally tagged,
    /// variant names as written).
    #[test]
    fn json_shape_matches_cast_type() {
        let cases = [
            (CosmType::Int, json!("Int")),
            (CosmType::Any, json!("Any")),
            (CosmType::Array(Box::new(CosmType::Float)), json!({"Array": "Float"})),
            (CosmType::Struct("Point".into()), json!({"Struct": "Point"})),
            (CosmType::Tuple(vec![CosmType::Int, CosmType::Bool]), json!({"Tuple": ["Int", "Bool"]})),
            (
                CosmType::Lambda { params: vec![CosmType::Int], returns: Box::new(CosmType::Null) },
                json!({"Lambda": {"params": ["Int"], "returns": "Null"}}),
            ),
            (CosmType::TypeRef("Shape".into()), json!({"TypeRef": "Shape"})),
        ];
        for (ty, expected) in cases {
            assert_eq!(serde_json::to_value(&ty).unwrap(), expected);
            assert_eq!(serde_json::from_value::<CosmType>(expected).unwrap(), ty);
        }
    }
}
