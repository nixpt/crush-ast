use std::rc::Rc;
use std::cell::RefCell;
use crush_vm::host::{HostCap, HostCapSpec};
use crush_vm::vm::Value;
use caison::parser::CaisonParser;
use caison::{CaisonNode, CaisonValue};

/// Exposes the `cson.parse` capability to Crush VM.
///
/// The capability name (`cson.parse`) is intentionally left as-is even
/// though the format is now called CAISON — renaming the Crush-language
/// capability surface is a separate decision, not part of CRUSH-CAISON-1.
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

        let mut parser = CaisonParser::new(&input);
        let doc = parser.parse()?;

        Ok(Some(node_to_value(doc.root)))
    }
}

fn node_to_value(node: CaisonNode) -> Value {
    match node.value {
        CaisonValue::String(s) => Value::Str(s),
        CaisonValue::Number(n) => Value::Float(n),
        CaisonValue::Boolean(b) => Value::Bool(b),
        CaisonValue::Null => Value::Null,
        CaisonValue::Synthesize(s) => {
            Value::Str(format!("<< SYNTHESIZE: {} >>", s))
        },
        CaisonValue::Array(arr) => {
            let vec: Vec<Value> = arr.into_iter().map(node_to_value).collect();
            Value::Array(Rc::new(RefCell::new(vec)))
        }
        CaisonValue::Object(obj) => {
            let mut map = std::collections::HashMap::new();
            for (k, v) in obj {
                map.insert(k, node_to_value(v));
            }
            Value::Map(Rc::new(RefCell::new(map)))
        }
    }
}
