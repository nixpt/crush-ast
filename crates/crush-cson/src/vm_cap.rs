use std::rc::Rc;
use std::cell::RefCell;
use crush_vm::host::{HostCap, HostCapSpec};
use crush_vm::vm::Value;
use crate::parser::CsonParser;
use crate::{CsonNode, CsonValue};

/// Exposes the `cson.parse` capability to Crush VM.
pub struct CsonParseCap;

impl HostCap for CsonParseCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "cson.parse".to_string(),
            argc: Some(1),
            returns: true,
        }
    }

    fn call(&self, mut args: Vec<Value>) -> Result<Option<Value>, String> {
        let input = match args.pop() {
            Some(Value::Str(s)) => s,
            _ => return Err("cson.parse expects a string argument".to_string()),
        };

        let mut parser = CsonParser::new(&input);
        let doc = parser.parse()?;

        Ok(Some(node_to_value(doc.root)))
    }
}

fn node_to_value(node: CsonNode) -> Value {
    match node.value {
        CsonValue::String(s) => Value::Str(s),
        CsonValue::Number(n) => Value::Float(n),
        CsonValue::Boolean(b) => Value::Bool(b),
        CsonValue::Null => Value::Null,
        CsonValue::Synthesize(s) => {
            Value::Str(format!("<< SYNTHESIZE: {} >>", s))
        },
        CsonValue::Array(arr) => {
            let vec: Vec<Value> = arr.into_iter().map(node_to_value).collect();
            Value::Array(Rc::new(RefCell::new(vec)))
        }
        CsonValue::Object(obj) => {
            let mut map = std::collections::HashMap::new();
            for (k, v) in obj {
                map.insert(k, node_to_value(v));
            }
            Value::Map(Rc::new(RefCell::new(map)))
        }
    }
}
