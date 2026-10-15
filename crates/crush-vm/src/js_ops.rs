//! The `js.*` capabilities on crush-vm values.
//!
//! The semantics live in [`crate::js_core`] (shared verbatim with the AOT
//! Rust runtime through [`JS_CORE_SRC`]); this module only converts
//! [`Value`]s to [`JsNode`]s and back.

use std::borrow::Cow;

use crate::js_core::{self, JsNode};
use crate::vm::{Value, VmError};

/// The text of `js_core.rs`, embedded by the AOT Rust code generator so the
/// generated runtime runs the same implementation as the VM.
pub const JS_CORE_SRC: &str = include_str!("js_core.rs");

fn unsupported(msg: String) -> VmError {
    VmError::Unsupported(msg)
}

/// A value as JS sees it; strings are borrowed, containers expanded
/// `expand` levels deep.
fn node(v: &Value, expand: usize) -> JsNode<'_> {
    match v {
        Value::Str(s) => JsNode::Str(Cow::Borrowed(s)),
        other => owned_node(other, expand),
    }
}

fn owned_node(v: &Value, expand: usize) -> JsNode<'static> {
    match v {
        Value::Null => JsNode::Null,
        Value::Bool(b) => JsNode::Bool(*b),
        Value::Int(i) => JsNode::Int(*i),
        Value::Float(f) => JsNode::Float(*f),
        Value::Str(s) => JsNode::Str(Cow::Owned(s.clone())),
        Value::Array(a) => {
            let items = a.borrow();
            JsNode::Array {
                id: std::rc::Rc::as_ptr(a) as *const () as usize,
                len: items.len(),
                items: (expand > 0).then(|| items.iter().take(js_core::ITEMS).map(|x| owned_node(x, expand - 1)).collect()),
            }
        }
        Value::Map(m) => {
            let entries = m.borrow();
            JsNode::Object {
                id: std::rc::Rc::as_ptr(m) as *const () as usize,
                len: entries.len(),
                entries: (expand > 0).then(|| entries.iter().map(|(k, x)| (k.clone(), owned_node(x, expand - 1))).collect()),
            }
        }
        other => JsNode::Other(other.type_name()),
    }
}

fn value(n: JsNode) -> Value {
    match n {
        JsNode::Null => Value::Null,
        JsNode::Bool(b) => Value::Bool(b),
        JsNode::Int(i) => Value::Int(i),
        JsNode::Float(f) => Value::Float(f),
        JsNode::Str(s) => Value::Str(s.into_owned()),
        // `add` only returns primitives.
        JsNode::Array { .. } | JsNode::Object { .. } | JsNode::Other(_) => Value::Null,
    }
}

/// `js.add(a, b)`: JS `+`.
pub fn add(a: &Value, b: &Value) -> Result<Value, VmError> {
    if let (Value::Str(x), Value::Str(y)) = (a, b) {
        return Ok(Value::Str(format!("{x}{y}")));
    }
    js_core::add(&node(a, 0), &node(b, 0)).map(value).map_err(|e| match e.as_str() {
        "arithmetic overflow" => VmError::ArithmeticOverflow,
        _ => unsupported(e),
    })
}

/// `js.truthy(v)`: JS truthiness.
pub fn truthy(v: &Value) -> Value {
    Value::Bool(js_core::truthy(&node(v, 0)))
}

/// `js.loose_eq(a, b)`: JS `==`.
pub fn loose_eq(a: &Value, b: &Value) -> Result<Value, VmError> {
    js_core::loose_eq(&node(a, 0), &node(b, 0)).map(Value::Bool).map_err(unsupported)
}

#[cfg(test)]
mod tests {
    //! Expected strings come from node v26 (`node -e …`), never by hand.
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn s(x: &str) -> Value {
        Value::Str(x.to_string())
    }
    fn arr(items: Vec<Value>) -> Value {
        Value::Array(Rc::new(RefCell::new(items)))
    }
    fn obj(entries: Vec<(&str, Value)>) -> Value {
        Value::Map(Rc::new(RefCell::new(entries.into_iter().map(|(k, v)| (k.to_string(), v)).collect())))
    }
    fn text(v: Value) -> String {
        match v {
            Value::Str(s) => s,
            other => panic!("expected a string, got {other:?}"),
        }
    }

    #[test]
    fn add_concatenates_with_js_number_text() {
        // node -e "console.log(['x'+6.0, 6.0+'x', 'a'+1e21, 'a'+0.1*3, 'a'+-0, 'a'+null, 'a'+true, 1.5+'', 'n'+1e-7].join('|'))"
        let got: Vec<String> = [
            (s("x"), Value::Float(6.0)),
            (Value::Float(6.0), s("x")),
            (s("a"), Value::Float(1e21)),
            (s("a"), Value::Float(0.1 * 3.0)),
            (s("a"), Value::Float(-0.0)),
            (s("a"), Value::Null),
            (s("a"), Value::Bool(true)),
            (Value::Float(1.5), s("")),
            (s("n"), Value::Float(1e-7)),
        ]
        .into_iter()
        .map(|(a, b)| text(add(&a, &b).unwrap()))
        .collect();
        assert_eq!(got.join("|"), "x6|6x|a1e+21|a0.30000000000000004|a0|anull|atrue|1.5|n1e-7");
    }

    #[test]
    fn number_text_matches_node() {
        // node -e "console.log([11.2613677978515625, 5e-324, 1.7976931348623157e308, 123e-20, 2**53+2, 0.000001, 1e-7, 1e21, 123456789012345680000, -1.5e-7].map(String).join(' '))"
        // (11.2613677978515625 is an exact tie: JS takes the even digit; Rust's
        // shortest Display rounds it up to …563.)
        let tie: f64 = "11.2613677978515625".parse().unwrap();
        let got: Vec<String> = [tie, 5e-324, 1.7976931348623157e308, 123e-20, 9007199254740994.0, 0.000001, 1e-7, 1e21, 123456789012345680000.0, -1.5e-7]
            .iter().map(|f| js_core::number_text(*f)).collect();
        assert_eq!(got.join(" "), "11.261367797851562 5e-324 1.7976931348623157e+308 1.23e-18 9007199254740994 0.000001 1e-7 1e+21 123456789012345680000 -1.5e-7");
    }

    #[test]
    fn add_of_ints_past_2_pow_53_prints_as_js_doubles() {
        // node -e "console.log('x' + 7774233818054199000)"
        assert_eq!(text(add(&s("x"), &Value::Int(7_774_233_818_054_199_296)).unwrap()), "x7774233818054199000");
    }

    #[test]
    fn add_numbers_stays_numeric() {
        assert_eq!(add(&Value::Int(2), &Value::Int(3)).unwrap(), Value::Int(5));
        assert_eq!(add(&Value::Int(2), &Value::Float(0.5)).unwrap(), Value::Float(2.5));
        // node: true + 1 === 2, null + 1 === 1
        assert_eq!(add(&Value::Bool(true), &Value::Int(1)).unwrap(), Value::Int(2));
        assert_eq!(add(&Value::Null, &Value::Int(1)).unwrap(), Value::Int(1));
        assert!(matches!(add(&Value::Int(i64::MAX), &Value::Int(1)), Err(VmError::ArithmeticOverflow)));
    }

    #[test]
    fn add_with_an_object_is_loud() {
        let err = add(&s("a"), &arr(vec![])).unwrap_err().to_string();
        assert!(err.contains("not supported") && err.contains("ToPrimitive"), "{err}");
        assert!(add(&obj(vec![]), &Value::Int(1)).is_err());
    }

    #[test]
    fn truthiness_matches_js() {
        // node -e "console.log([0, -0, NaN, '', null, false, 1, 'a', [], {}].map(Boolean).join())"
        let vals = [
            Value::Int(0), Value::Float(-0.0), Value::Float(f64::NAN), s(""), Value::Null, Value::Bool(false),
            Value::Int(1), s("a"), arr(vec![]), obj(vec![]),
        ];
        let got: Vec<String> = vals.iter().map(|v| truthy(v).to_string()).collect();
        assert_eq!(got.join(","), "false,false,false,false,false,false,true,true,true,true");
    }

    #[test]
    fn loose_equality_matches_js() {
        // node -e "console.log(['1'==1, 1=='1.0', ''==0, ' \t\n'==0, '0x1f'==31, '1e3'==1000, true==1, true=='1', false=='', null==0, null==null, NaN==NaN, 'Infinity'==1/0, '1_0'==10, '.5'==0.5, '5.'==5, '+5'==5, '0b101'==5, '-0x1'==-1, 'a'=='a', 2==2.0].join())"
        let cases = [
            (s("1"), Value::Int(1)), (Value::Int(1), s("1.0")), (s(""), Value::Int(0)), (s(" \t\n"), Value::Int(0)),
            (s("0x1f"), Value::Int(31)), (s("1e3"), Value::Int(1000)), (Value::Bool(true), Value::Int(1)),
            (Value::Bool(true), s("1")), (Value::Bool(false), s("")), (Value::Null, Value::Int(0)), (Value::Null, Value::Null),
            (Value::Float(f64::NAN), Value::Float(f64::NAN)), (s("Infinity"), Value::Float(f64::INFINITY)),
            (s("1_0"), Value::Int(10)), (s(".5"), Value::Float(0.5)), (s("5."), Value::Int(5)), (s("+5"), Value::Int(5)),
            (s("0b101"), Value::Int(5)), (s("-0x1"), Value::Int(-1)), (s("a"), s("a")), (Value::Int(2), Value::Float(2.0)),
        ];
        let got: Vec<String> = cases.iter().map(|(a, b)| loose_eq(a, b).unwrap().to_string()).collect();
        assert_eq!(
            got.join(","),
            "true,true,true,true,true,true,true,true,true,false,true,false,true,false,true,true,true,true,false,true,true"
        );
    }

    #[test]
    fn loose_equality_of_objects_is_identity_and_object_vs_primitive_is_loud() {
        let a = arr(vec![Value::Int(1)]);
        assert_eq!(loose_eq(&a, &a.clone()).unwrap(), Value::Bool(true));
        assert_eq!(loose_eq(&a, &arr(vec![Value::Int(1)])).unwrap(), Value::Bool(false));
        assert_eq!(loose_eq(&a, &Value::Null).unwrap(), Value::Bool(false));
        assert!(loose_eq(&a, &Value::Int(1)).unwrap_err().to_string().contains("not supported"));
    }
}
