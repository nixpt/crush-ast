//! VM adapter for the pure shared kernels, never an ambient capability.
use crate::{
    rust_methods_core::{self, Input, Output},
    vm::Value,
};
pub const CORE_SOURCE: &str = include_str!("rust_methods_core.rs");
pub fn call(name: &str, args: &[Value]) -> Result<Value, String> {
    let strings: Vec<_> = args
        .iter()
        .map(|v| {
            if name.starts_with("str.") {
                Some(v.to_string())
            } else {
                None
            }
        })
        .collect();
    let inputs: Vec<_> = args
        .iter()
        .zip(&strings)
        .map(|(v, s)| {
            if let Some(s) = s {
                return Input::Text(s);
            }
            match v {
                Value::Int(i) => Input::Int(*i),
                Value::Float(f) => Input::Float(*f),
                Value::Bool(b) => Input::Bool(*b),
                Value::Str(s) => Input::Text(s),
                Value::Null => Input::Null,
                _ => Input::Invalid,
            }
        })
        .collect();
    Ok(match rust_methods_core::call(name, &inputs)? {
        Output::Int(i) => Value::Int(i),
        Output::Float(f) => Value::Float(f),
        Output::Bool(b) => Value::Bool(b),
        Output::Text(s) => Value::Str(s),
    })
}
