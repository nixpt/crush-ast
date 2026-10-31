//! `lang.marshal` (CRUSH-272): the value boundary of an inline-lowered
//! polyglot block.
//!
//! A `@python` / `@javascript` block that runs in a subprocess sees copies of
//! its inputs and hands back copies of its outputs: both cross as JSON. A block
//! lowered inline (crush-lang-sdk's `PolyglotMode::Lower`) runs in the same VM,
//! where arrays and maps are shared by reference — so without this copy a
//! block's `xs.append(4)` would change the Crush variable `xs`, which the
//! subprocess never does. `lang.marshal(value, name)` reproduces the JSON
//! boundary: a deep copy with JSON's shapes (every sequence becomes an array,
//! a record becomes a map), and a loud error for a value JSON can't carry
//! (a function, a handle, a non-finite float), naming the variable.

use std::cell::RefCell;
use std::rc::Rc;

use crate::vm::{Value, VmError};

/// Copy `value` across a polyglot block boundary as JSON would; `name` is the
/// variable, for the error.
pub fn marshal_value(value: &Value, name: &str) -> Result<Value, VmError> {
    let refuse = |what: &str| {
        Err(VmError::Unsupported(format!(
            "polyglot block variable `{name}` holds {what}, which can't cross a polyglot \
             block boundary (values cross as JSON: null, bool, int, float, str, arrays, maps)"
        )))
    };
    let seq = |items: &mut dyn Iterator<Item = &Value>| -> Result<Value, VmError> {
        let copied = items.map(|v| marshal_value(v, name)).collect::<Result<Vec<_>, _>>()?;
        Ok(Value::Array(Rc::new(RefCell::new(copied))))
    };
    match value {
        Value::Null | Value::Bool(_) | Value::Int(_) | Value::Str(_) => Ok(value.clone()),
        Value::Float(f) if f.is_finite() => Ok(value.clone()),
        Value::Float(f) => refuse(&format!("the non-finite float {f}")),
        Value::Cell(c) => marshal_value(&c.borrow(), name),
        Value::Array(a) => seq(&mut a.borrow().iter()),
        Value::Vector(a) => seq(&mut a.borrow().iter()),
        Value::Set(a) => seq(&mut a.borrow().iter()),
        Value::List(l) => seq(&mut l.borrow().iter()),
        Value::Tuple(t) => seq(&mut t.iter()),
        Value::Map(m) => {
            let mut out = crate::vm::MapStore::new();
            for (k, v) in m.borrow().iter() {
                out.insert(k.clone(), marshal_value(v, name)?);
            }
            Ok(Value::Map(Rc::new(RefCell::new(out))))
        }
        Value::Record(r) => {
            let mut out = crate::vm::MapStore::new();
            for (k, v) in r.fields() {
                out.insert(k, marshal_value(&v, name)?);
            }
            Ok(Value::Map(Rc::new(RefCell::new(out))))
        }
        Value::Func(_) => refuse("a function"),
        Value::Error(_) => refuse("an error value"),
        Value::Bytes(_) => refuse("bytes"),
        Value::Handle(_) => refuse("a task handle"),
        Value::Foreign(_) => refuse("a foreign object"),
        Value::Kind(_) => refuse("a kind"),
    }
}

/// The `lang.marshal` capability: `(value, name)`.
pub(crate) fn marshal_cap(args: &[Value]) -> Result<Option<Value>, VmError> {
    let name = match &args[1] {
        Value::Str(s) => s.as_str(),
        _ => "?",
    };
    marshal_value(&args[0], name).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arr(xs: Vec<Value>) -> Value {
        Value::Array(Rc::new(RefCell::new(xs)))
    }

    #[test]
    fn copies_do_not_share() {
        let inner = arr(vec![Value::Int(1)]);
        let outer = arr(vec![inner.clone()]);
        let copy = marshal_value(&outer, "xs").unwrap();
        if let Value::Array(a) = &inner {
            a.borrow_mut().push(Value::Int(2));
        }
        assert_eq!(copy, arr(vec![arr(vec![Value::Int(1)])]));
    }

    #[test]
    fn tuples_become_arrays_and_floats_stay_floats() {
        let t = Value::Tuple(vec![Value::Float(2.0), Value::Str("a".into())]);
        let copy = marshal_value(&t, "t").unwrap();
        assert!(matches!(&copy, Value::Array(a) if matches!(a.borrow()[0], Value::Float(_))));
    }

    #[test]
    fn non_json_values_are_refused_by_name() {
        let e = marshal_value(&Value::Float(f64::NAN), "x").unwrap_err().to_string();
        assert!(e.contains("`x`") && e.contains("non-finite"), "{e}");
        let e = marshal_value(&arr(vec![Value::Handle(1)]), "hs").unwrap_err().to_string();
        assert!(e.contains("`hs`") && e.contains("task handle"), "{e}");
    }
}
