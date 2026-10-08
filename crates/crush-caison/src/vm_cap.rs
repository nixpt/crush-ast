use std::rc::Rc;
use std::cell::RefCell;
use crush_vm::host::{HostCap, HostCapSpec};
use crush_vm::vm::Value;
use caison::parser::CaisonParser;
use caison::{CaisonNode, CaisonValue};

/// Capability name for CAISON parsing.
pub const CAP_NAME: &str = "caison.parse";
/// Deprecated pre-rename capability name (CSON → CAISON); same handler.
/// Removed in 0.5.
pub const DEPRECATED_CAP_NAME: &str = "cson.parse";

/// Exposes the `caison.parse` capability to Crush VM.
///
/// Registered under [`CAP_NAME`] by default; [`CaisonParseCap::deprecated_alias`]
/// yields the same handler under the old `cson.parse` name.
pub struct CaisonParseCap {
    name: &'static str,
}

impl CaisonParseCap {
    pub fn new() -> Self {
        Self { name: CAP_NAME }
    }

    /// The same handler registered as the deprecated `cson.parse`.
    pub fn deprecated_alias() -> Self {
        Self { name: DEPRECATED_CAP_NAME }
    }
}

impl Default for CaisonParseCap {
    fn default() -> Self {
        Self::new()
    }
}

impl HostCap for CaisonParseCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: self.name.to_string(),
            argc: Some(1),
            returns: true,
        }
    }

    fn call(&self, mut args: Vec<Value>) -> Result<Option<Value>, String> {
        let input = match args.pop() {
            Some(Value::Str(s)) => s,
            _ => return Err(format!("{} expects a string argument", self.name)),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(cap: &CaisonParseCap, src: &str) -> Value {
        cap.call(vec![Value::Str(src.to_string())]).unwrap().unwrap()
    }

    #[test]
    fn both_names_parse_to_the_same_value() {
        let new = CaisonParseCap::new();
        let old = CaisonParseCap::deprecated_alias();
        assert_eq!(new.spec().name, "caison.parse");
        assert_eq!(old.spec().name, "cson.parse");
        let src = "name: \"crush\"\nport: 8080\nflags: [true, false]\n";
        // HashMap iteration order is unstable, so compare sorted entries.
        let entries = |v: Value| match v {
            Value::Map(m) => {
                let mut e: Vec<String> =
                    m.borrow().iter().map(|(k, v)| format!("{k}={v:?}")).collect();
                e.sort();
                e
            }
            other => panic!("expected a map, got {other:?}"),
        };
        let (a, b) = (entries(parse(&new, src)), entries(parse(&old, src)));
        assert_eq!(a.len(), 3, "{a:?}");
        assert_eq!(a, b);
    }

    #[test]
    fn non_string_argument_errors_with_the_registered_name() {
        let e = CaisonParseCap::deprecated_alias().call(vec![Value::Null]).unwrap_err();
        assert!(e.starts_with("cson.parse"), "{e}");
    }
}
