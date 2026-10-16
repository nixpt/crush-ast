//! Runtime adapter for the shared typed Rust Debug kernel.
use crate::rust_debug_core::{self as core, Atom, Error, View};
use crate::vm::Value;

pub const CORE_SOURCE: &str = include_str!("rust_debug_core.rs");
impl View for Value {
    fn atom(&self) -> Atom<'_> {
        match self {
            Self::Null => Atom::Unit,
            Self::Bool(v) => Atom::Bool(*v),
            Self::Int(v) => Atom::Int(*v),
            Self::Float(v) => Atom::Float(*v),
            Self::Str(v) => Atom::Text(v),
            Self::Array(v) => Atom::Seq(std::rc::Rc::as_ptr(v) as usize),
            _ => Atom::Other,
        }
    }
    fn with_items<R>(&self, visit: impl FnOnce(&[Self]) -> Result<R, Error>) -> Result<R, Error> {
        match self {
            Self::Array(v) => visit(&v.try_borrow().map_err(|_| Error::Borrow)?),
            _ => Err(Error::Mismatch),
        }
    }
}

pub fn format(value: &Value, descriptor: &Value) -> Result<Value, String> {
    let Value::Str(desc) = descriptor else {
        return Err(Error::Descriptor.to_string());
    };
    let ty = core::descriptor(desc).map_err(|e| e.to_string())?;
    core::format(value, &ty)
        .map(Value::Str)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};
    fn text(s: &str) -> Value {
        Value::Str(s.into())
    }
    fn array(v: Vec<Value>) -> Value {
        Value::Array(Rc::new(RefCell::new(v)))
    }
    fn debug(v: &Value, d: &str) -> Result<String, String> {
        match format(v, &text(d))? {
            Value::Str(s) => Ok(s),
            _ => unreachable!(),
        }
    }
    #[test]
    fn scalar_types_quote_and_escape_without_guessing_runtime_representation() {
        for (v, d, expected) in [
            (
                text("é\"\\\n\t\0"),
                r#"{"kind":"str"}"#,
                format!("{:?}", "é\"\\\n\t\0"),
            ),
            (text("é"), r#"{"kind":"char"}"#, "'é'".into()),
            (Value::Null, r#"{"kind":"unit"}"#, "()".into()),
            (Value::Bool(true), r#"{"kind":"bool"}"#, "true".into()),
            (Value::Int(-128), r#"{"kind":"i8"}"#, "-128".into()),
            (Value::Float(-0.0), r#"{"kind":"f64"}"#, "-0.0".into()),
            (Value::Float(f64::NAN), r#"{"kind":"f64"}"#, "NaN".into()),
            (
                Value::Float(f64::INFINITY),
                r#"{"kind":"f64"}"#,
                "inf".into(),
            ),
        ] {
            assert_eq!(debug(&v, d).unwrap(), expected);
        }
    }
    #[test]
    fn rejects_descriptor_value_disagreement_and_bad_integer_widths() {
        for (v, d) in [
            (Value::Int(256), r#"{"kind":"u8"}"#),
            (Value::Int(-1), r#"{"kind":"u64"}"#),
            (Value::Int(6), r#"{"kind":"f64"}"#),
            (text("two"), r#"{"kind":"char"}"#),
            (text(""), r#"{"kind":"char"}"#),
            (text("x"), r#"{"kind":"unit"}"#),
            (Value::Float(1.0), r#"{"kind":"i64"}"#),
        ] {
            assert!(debug(&v, d).unwrap_err().contains("does not match"));
        }
        assert_eq!(
            core::format(
                &Value::Int(1),
                &core::Type::Int {
                    bits: 0,
                    signed: true
                }
            ),
            Err(Error::Descriptor)
        );
        assert_eq!(
            core::format(
                &Value::Int(1),
                &core::Type::Int {
                    bits: 128,
                    signed: false
                }
            ),
            Err(Error::Descriptor)
        );
    }
    #[test]
    fn descriptor_grammar_is_strict_bounded_json() {
        for d in [
            "null",
            "{}",
            r#"{"kind":"f32"}"#,
            r#"{"kind":"i128"}"#,
            r#"{"kind":"str","kind":"char"}"#,
            r#"{"kind":"str","extra":1}"#,
            r#"{"kind":"seq"}"#,
            r#"{"kind":"str","item":{"kind":"str"}}"#,
            r#"{"kind":"str",}"#,
            r#"{"kind":"str"} trailing"#,
            "{\u{000b}\"kind\":\"str\"}",
            r#"{"kind":"\u0073tr"}"#,
        ] {
            assert!(core::descriptor(d).is_err(), "{d}");
        }
        assert!(core::descriptor(&" ".repeat(core::MAX_DESCRIPTOR + 1)).is_err());
        let deep = format!(
            "{}{{\"kind\":\"str\"}}{}",
            "{\"kind\":\"seq\",\"item\":".repeat(18),
            "}".repeat(18)
        );
        assert_eq!(core::descriptor(&deep), Err(Error::Depth));
        assert!(core::descriptor("{ \"item\" : {\"kind\":\"char\"}, \"kind\" : \"seq\" }").is_ok());
    }
    #[test]
    fn sequences_are_typed_recursively_and_shared_acyclic_values_are_allowed() {
        let shared = array(vec![text("a"), text("é")]);
        let root = array(vec![shared.clone(), shared]);
        let d = r#"{"kind":"seq","item":{"kind":"seq","item":{"kind":"char"}}}"#;
        assert_eq!(debug(&root, d).unwrap(), "[['a', 'é'], ['a', 'é']]");
        assert!(
            debug(
                &array(vec![text("x"), Value::Int(1)]),
                r#"{"kind":"seq","item":{"kind":"str"}}"#
            )
            .is_err()
        );
    }
    #[test]
    fn traversal_detects_cycles_and_borrow_conflicts_without_recursive_cloning() {
        let v = Rc::new(RefCell::new(Vec::new()));
        let root = Value::Array(v.clone());
        v.borrow_mut().push(root.clone());
        let d = r#"{"kind":"seq","item":{"kind":"seq","item":{"kind":"str"}}}"#;
        assert!(debug(&root, d).unwrap_err().contains("cyclic"));
        v.borrow_mut().clear();
        let _borrow = v.borrow_mut();
        assert!(
            debug(&root, r#"{"kind":"seq","item":{"kind":"str"}}"#)
                .unwrap_err()
                .contains("borrowed")
        );
    }
    #[test]
    fn traversal_and_output_quotas_fail_explicitly() {
        let huge = array(vec![Value::Int(0); core::MAX_ITEMS]);
        assert!(
            debug(&huge, r#"{"kind":"seq","item":{"kind":"i32"}}"#)
                .unwrap_err()
                .contains("item quota")
        );
        let huge = text(&"\0".repeat(core::MAX_OUTPUT));
        assert!(
            debug(&huge, r#"{"kind":"str"}"#)
                .unwrap_err()
                .contains("output byte quota")
        );
        let mut value = Value::Int(0);
        let mut ty = core::Type::Int {
            bits: 32,
            signed: true,
        };
        for _ in 0..18 {
            value = array(vec![value]);
            ty = core::Type::Seq(Box::new(ty));
        }
        assert_eq!(core::format(&value, &ty), Err(Error::Depth));
    }
}
