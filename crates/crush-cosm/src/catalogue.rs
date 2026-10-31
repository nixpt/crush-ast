//! The closed strategy catalogue as text: the spelling of every strategy, and
//! parsing from that spelling with an error that lists the valid values.
//!
//! This is what a `@kind(resolution: c3, …)` front end needs: the spelling
//! here is the JSON spelling, and an unknown value is a [`CatalogueError`]
//! naming the dimension and every valid value.

use crate::schema::{Equality, Receiver, Resolution, Satisfaction, Semantics, Storage, ValueMode};
use std::fmt;
use std::str::FromStr;

/// An unknown strategy name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogueError {
    /// The dimension (`"resolution"`, `"receiver"`, …).
    pub dimension: &'static str,
    /// What was written.
    pub got: String,
    /// Every valid spelling, in catalogue order.
    pub valid: &'static [&'static str],
}

impl fmt::Display for CatalogueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown {} strategy `{}`; expected one of: {}",
            self.dimension,
            self.got,
            self.valid.join(", ")
        )
    }
}

impl std::error::Error for CatalogueError {}

/// A strategy dimension whose every value can be written as a bare name.
pub trait Strategy: Sized + Copy + PartialEq + 'static {
    /// The `@kind` / JSON key of this dimension.
    const DIMENSION: &'static str;
    /// Every value, in catalogue order.
    const ALL: &'static [Self];
    /// Every spelling, parallel to [`Strategy::ALL`].
    const NAMES: &'static [&'static str];

    /// This value's spelling (identical to its JSON spelling).
    fn as_str(self) -> &'static str;

    /// Parse a spelling, or say what's valid.
    fn parse(s: &str) -> Result<Self, CatalogueError> {
        Self::ALL
            .iter()
            .copied()
            .find(|v| v.as_str() == s)
            .ok_or_else(|| CatalogueError {
                dimension: Self::DIMENSION,
                got: s.to_string(),
                valid: Self::NAMES,
            })
    }
}

macro_rules! strategy {
    ($ty:ty, $dim:literal, [$($variant:expr => $name:literal),* $(,)?]) => {
        impl Strategy for $ty {
            const DIMENSION: &'static str = $dim;
            const ALL: &'static [Self] = &[$($variant),*];
            const NAMES: &'static [&'static str] = &[$($name),*];
            fn as_str(self) -> &'static str {
                let i = Self::ALL.iter().position(|v| *v == self).expect("ALL lists every variant");
                Self::NAMES[i]
            }
        }
        impl FromStr for $ty {
            type Err = CatalogueError;
            fn from_str(s: &str) -> Result<Self, CatalogueError> {
                <$ty as Strategy>::parse(s)
            }
        }
        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

strategy!(Resolution, "resolution", [
    Resolution::None => "none",
    Resolution::Single => "single",
    Resolution::C3 => "c3",
    Resolution::PrototypeChain => "prototype_chain",
    Resolution::EmbeddingPromotion => "embedding_promotion",
]);

strategy!(Receiver, "receiver", [
    Receiver::DynamicThis => "dynamic_this",
    Receiver::BoundSelf => "bound_self",
    Receiver::BoundCls => "bound_cls",
    Receiver::ExplicitValue => "explicit_value",
    Receiver::ExplicitRef => "explicit_ref",
    Receiver::ExplicitRefMut => "explicit_ref_mut",
]);

strategy!(ValueMode, "value", [
    ValueMode::Copy => "copy",
    ValueMode::Move => "move",
    ValueMode::CloneExplicit => "clone_explicit",
]);

strategy!(Semantics, "semantics", [
    Semantics::Reference => "reference",
    Semantics::Value(ValueMode::Copy) => "value(copy)",
    Semantics::Value(ValueMode::Move) => "value(move)",
    Semantics::Value(ValueMode::CloneExplicit) => "value(clone_explicit)",
]);

strategy!(Equality, "equality", [
    Equality::Identity => "identity",
    Equality::Structural => "structural",
]);

strategy!(Satisfaction, "satisfaction", [
    Satisfaction::Nominal => "nominal",
    Satisfaction::Structural => "structural",
]);

/// Storage strategy names (the variants carry field lists, so they are not a
/// [`Strategy`]; [`Storage::from_name`] builds one with no fields yet).
pub const STORAGE_NAMES: &[&str] = &["sealed", "open_dict", "open_dict_with_slots"];

/// Construction strategy names (each but `literal` takes functions, which a
/// front end supplies).
pub const CONSTRUCT_NAMES: &[&str] = &["literal", "ctor", "js_new", "py_new_init"];

impl Storage {
    /// The strategy name of this storage.
    pub fn strategy_name(&self) -> &'static str {
        match self {
            Storage::Sealed { .. } => "sealed",
            Storage::OpenDict => "open_dict",
            Storage::OpenDictWithSlots { .. } => "open_dict_with_slots",
        }
    }

    /// Storage from its strategy name, with no fields/slots yet.
    pub fn from_name(s: &str) -> Result<Self, CatalogueError> {
        match s {
            "sealed" => Ok(Storage::Sealed { fields: Vec::new() }),
            "open_dict" => Ok(Storage::OpenDict),
            "open_dict_with_slots" => Ok(Storage::OpenDictWithSlots { slots: Vec::new() }),
            _ => Err(CatalogueError {
                dimension: "storage",
                got: s.to_string(),
                valid: STORAGE_NAMES,
            }),
        }
    }
}

impl crate::schema::Construct {
    /// The strategy name of this construction.
    pub fn strategy_name(&self) -> &'static str {
        use crate::schema::Construct::*;
        match self {
            Literal => "literal",
            Ctor { .. } => "ctor",
            JsNew { .. } => "js_new",
            PyNewInit { .. } => "py_new_init",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip<T: Strategy + PartialEq + fmt::Debug + serde::Serialize>() {
        assert_eq!(T::ALL.len(), T::NAMES.len());
        for (v, name) in T::ALL.iter().zip(T::NAMES) {
            assert_eq!(v.as_str(), *name);
            assert_eq!(T::parse(name).unwrap(), *v);
        }
    }

    #[test]
    fn every_strategy_spelling_parses_back() {
        roundtrip::<Resolution>();
        roundtrip::<Receiver>();
        roundtrip::<ValueMode>();
        roundtrip::<Semantics>();
        roundtrip::<Equality>();
        roundtrip::<Satisfaction>();
    }

    #[test]
    fn spelling_matches_json_for_unit_strategies() {
        for r in Resolution::ALL {
            assert_eq!(
                serde_json::to_value(r).unwrap(),
                serde_json::json!(r.as_str())
            );
        }
        for r in Receiver::ALL {
            assert_eq!(
                serde_json::to_value(r).unwrap(),
                serde_json::json!(r.as_str())
            );
        }
        for v in ValueMode::ALL {
            assert_eq!(
                serde_json::to_value(Semantics::Value(*v)).unwrap(),
                serde_json::json!({ "value": v.as_str() })
            );
        }
    }

    #[test]
    fn unknown_value_lists_the_catalogue() {
        let err = "mro".parse::<Resolution>().unwrap_err();
        assert_eq!(
            err.to_string(),
            "unknown resolution strategy `mro`; expected one of: none, single, c3, prototype_chain, embedding_promotion"
        );
        let err = Storage::from_name("closed").unwrap_err();
        assert_eq!(err.valid, STORAGE_NAMES);
        assert!("bound_cls".parse::<Receiver>().is_ok());
        assert!("explicit_ref_mut".parse::<Receiver>().is_ok());
    }
}
