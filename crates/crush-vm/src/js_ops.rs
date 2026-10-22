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

/// `js.strict_eq(a, b)`: JS `===`.
pub fn strict_eq(a: &Value, b: &Value) -> Result<Value, VmError> {
    js_core::strict_eq(&node(a, 0), &node(b, 0)).map(Value::Bool).map_err(unsupported)
}

/// `js.format(args...)`: the line `console.log(args...)` prints.
pub fn format(args: &[Value]) -> Result<Value, VmError> {
    let nodes: Vec<JsNode> = args.iter().map(|a| node(a, js_core::EXPAND)).collect();
    js_core::format(&nodes).map(Value::Str).map_err(unsupported)
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

    #[test]
    fn strict_equality_matches_js() {
        // node -e "console.log([1===1, 1===2, 1===1.0, '1'===1, 1==='1', 'a'==='a', 'a'==='b', true===1, true===true, false===0, null===null, null===0, null==='', NaN===NaN, '1'!=='1', 1!=='1'].join())"
        let cases = [
            (Value::Int(1), Value::Int(1)), (Value::Int(1), Value::Int(2)), (Value::Int(1), Value::Float(1.0)),
            (s("1"), Value::Int(1)), (Value::Int(1), s("1")), (s("a"), s("a")), (s("a"), s("b")),
            (Value::Bool(true), Value::Int(1)), (Value::Bool(true), Value::Bool(true)), (Value::Bool(false), Value::Int(0)),
            (Value::Null, Value::Null), (Value::Null, Value::Int(0)), (Value::Null, s("")),
            (Value::Float(f64::NAN), Value::Float(f64::NAN)), (s("1"), s("1")), (Value::Int(1), s("1")),
        ];
        let got: Vec<String> = cases.iter().map(|(a, b)| strict_eq(a, b).unwrap().to_string()).collect();
        assert_eq!(
            got.join(","),
            "true,false,true,false,false,true,false,false,true,false,true,false,false,false,true,false"
        );
    }

    #[test]
    fn strict_equality_of_objects_is_identity() {
        let a = arr(vec![Value::Int(1)]);
        assert_eq!(strict_eq(&a, &a.clone()).unwrap(), Value::Bool(true));
        assert_eq!(strict_eq(&a, &arr(vec![Value::Int(1)])).unwrap(), Value::Bool(false));
        let o = obj(vec![("a", Value::Int(1))]);
        assert_eq!(strict_eq(&o, &o.clone()).unwrap(), Value::Bool(true));
        assert_eq!(strict_eq(&o, &obj(vec![("a", Value::Int(1))])).unwrap(), Value::Bool(false));
        // Unlike `==`, an object against a primitive is `false`, not a
        // ToPrimitive error.
        assert_eq!(strict_eq(&a, &Value::Int(1)).unwrap(), Value::Bool(false));
        assert_eq!(strict_eq(&a, &Value::Null).unwrap(), Value::Bool(false));
    }

    fn log(args: Vec<Value>) -> String {
        text(format(&args).unwrap())
    }

    #[test]
    fn format_matches_node_inspect() {
        // node -e "console.log([1, 'a', true, null], [6], {a: 1, b: 'two'}, [], {}, -0, [-0, 1.5, 1e21])"
        assert_eq!(
            log(vec![
                arr(vec![Value::Int(1), s("a"), Value::Bool(true), Value::Null]),
                arr(vec![Value::Float(6.0)]),
                obj(vec![("a", Value::Int(1)), ("b", s("two"))]),
                arr(vec![]),
                obj(vec![]),
                Value::Float(-0.0),
                arr(vec![Value::Float(-0.0), Value::Float(1.5), Value::Float(1e21)]),
            ]),
            "[ 1, 'a', true, null ] [ 6 ] { a: 1, b: 'two' } [] {} -0 [ -0, 1.5, 1e+21 ]"
        );
        // node -e "console.log('s', 1, 'x y')"
        assert_eq!(log(vec![s("s"), Value::Int(1), s("x y")]), "s 1 x y");
        assert_eq!(log(vec![]), "");
    }

    #[test]
    fn format_quotes_and_escapes_strings_like_node() {
        // node -e "console.log([\"it's\", 'say \"hi\"', 'both \\' \"', 'all \\' \" \`', 'tab\\there', 'nl\\n', 'back\\\\slash', '\\x1b[0m', 'cafe'])"
        assert_eq!(
            log(vec![arr(vec![
                s("it's"), s("say \"hi\""), s("both ' \""), s("all ' \" `"), s("tab\there"), s("nl\n"),
                s("back\\slash"), s("\x1b[0m"), s("cafe"),
            ])]),
            "[\n  \"it's\",\n  'say \"hi\"',\n  `both ' \"`,\n  'all \\' \" `',\n  'tab\\there',\n  'nl\\n',\n  'back\\\\slash',\n  '\\x1B[0m',\n  'cafe'\n]"
        );
        // node -e "console.log(['café', 'ñ', '日本'])"
        assert_eq!(log(vec![arr(vec![s("café"), s("ñ"), s("日本")])]), "[ 'café', 'ñ', '日本' ]");
    }

    #[test]
    fn format_keys_like_node() {
        // node -e "console.log({b: 1, 2: 'x', 'my-key': 3, 1: 'y', _ok: 4, '': 5})"
        assert_eq!(
            log(vec![obj(vec![
                ("b", Value::Int(1)), ("2", s("x")), ("my-key", Value::Int(3)), ("1", s("y")), ("_ok", Value::Int(4)), ("", Value::Int(5)),
            ])]),
            "{ '1': 'y', '2': 'x', b: 1, 'my-key': 3, _ok: 4, '': 5 }"
        );
    }

    #[test]
    fn format_depth_and_nesting_like_node() {
        // node -e "console.log({a: {b: {c: {d: 1}}}}, [[[[1]]]], [[[[]]]], {x: [1, {y: 2}]})"
        let deep = obj(vec![("a", obj(vec![("b", obj(vec![("c", obj(vec![("d", Value::Int(1))]))]))]))]);
        let deep_arr = arr(vec![arr(vec![arr(vec![arr(vec![Value::Int(1)])])])]);
        let deep_empty = arr(vec![arr(vec![arr(vec![arr(vec![])])])]);
        let mixed = obj(vec![("x", arr(vec![Value::Int(1), obj(vec![("y", Value::Int(2))])]))]);
        assert_eq!(
            log(vec![deep, deep_arr, deep_empty, mixed]),
            "{ a: { b: { c: [Object] } } } [ [ [ [Array] ] ] ] [ [ [ [] ] ] ] { x: [ 1, { y: 2 } ] }"
        );
    }

    #[test]
    fn format_breaks_long_lines_and_groups_like_node() {
        // node -e "console.log(Array.from({length: 30}, (_, i) => i * 7))"
        let nums = arr((0..30).map(|i| Value::Int(i * 7)).collect());
        assert_eq!(
            log(vec![nums]),
            "[\n    0,   7,  14,  21,  28,  35,  42,  49,\n   56,  63,  70,  77,  84,  91,  98, 105,\n  112, 119, 126, 133, 140, 147, 154, 161,\n  168, 175, 182, 189, 196, 203\n]"
        );
        // node -e "console.log({alpha: 'aaaaaaaaaaaaaaaa', beta: 'bbbbbbbbbbbbbbbbbbbb', gamma: 'cccccccccccccccccc', delta: 1})"
        let wide = obj(vec![
            ("alpha", s("aaaaaaaaaaaaaaaa")), ("beta", s("bbbbbbbbbbbbbbbbbbbb")), ("gamma", s("cccccccccccccccccc")), ("delta", Value::Int(1)),
        ]);
        assert_eq!(
            log(vec![wide]),
            "{\n  alpha: 'aaaaaaaaaaaaaaaa',\n  beta: 'bbbbbbbbbbbbbbbbbbbb',\n  gamma: 'cccccccccccccccccc',\n  delta: 1\n}"
        );
        // node -e "console.log(Array.from({length: 102}, (_, i) => i))"
        let many = arr((0..102).map(Value::Int).collect());
        assert_eq!(
            log(vec![many]),
            "[\n   0,  1,  2,  3,  4,  5,  6,  7,  8,  9, 10, 11,\n  12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,\n  24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35,\n  36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47,\n  48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59,\n  60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71,\n  72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83,\n  84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95,\n  96, 97, 98, 99,\n  ... 2 more items\n]"
        );
    }

    #[test]
    fn format_of_ints_past_2_pow_53_prints_js_doubles() {
        // node -e "console.log(7774233818054199000, [7774233818054199000, -9007199254740993], 9007199254740992)"
        let big = Value::Int(7_774_233_818_054_199_296);
        let line = log(vec![
            big.clone(),
            arr(vec![big.clone(), Value::Int(-9_007_199_254_740_993)]),
            Value::Int(9_007_199_254_740_992),
        ]);
        assert_eq!(line, "7774233818054199000 [ 7774233818054199000, -9007199254740992 ] 9007199254740992");
    }

    #[test]
    fn format_directives_and_cycles_are_loud() {
        assert!(format(&[s("%d items"), Value::Int(3)]).unwrap_err().to_string().contains("format directives"));
        // A lone string is printed as is, `%` and all (node: `100%d`).
        assert_eq!(log(vec![s("100%d")]), "100%d");
        let a = arr(vec![]);
        if let Value::Array(inner) = &a {
            inner.borrow_mut().push(a.clone());
        }
        assert!(format(&[a]).unwrap_err().to_string().contains("circular"));
    }
}
