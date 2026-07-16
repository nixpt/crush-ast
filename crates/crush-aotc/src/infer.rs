//! Type inference for the AOT compiler (Pathway 2 heuristics).
//!
//! We do a single forward pass over CASM bytecode and track the "likely type"
//! of each local variable.  This is intentionally lightweight — no dataflow
//! lattice, no fixpoint — because the optimizer in `crush-frontend` has already
//! done constant propagation.  We just need to know: is this slot *always* an
//! integer / *always* a float so we can emit a raw C scalar instead of a
//! `CrushValue`?

use std::collections::HashMap;

/// Inferred scalar type for a local variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferredType {
    /// Definitely `int64_t`.
    Int,
    /// Definitely `double`.
    Float,
    /// Could be either or unknown — emit `CrushValue`.
    Dynamic,
}

/// Result of a single-pass type inference over a CASM function body.
pub struct TypeMap {
    pub locals: HashMap<String, InferredType>,
}

impl TypeMap {
    /// Run forward inference over the CASM instructions.
    ///
    /// Strategy:
    /// - `push_int`  → stack top is `Int`
    /// - `push_float`→ stack top is `Float`
    /// - `store X`   → bind X to whatever is on the stack top
    /// - `load X`    → push the type bound to X (or Dynamic)
    /// - arithmetic  → if both operands are the same scalar type, result is
    ///                 that type; otherwise Dynamic
    /// - any branch / call → mark the top as Dynamic (conservative)
    pub fn infer(body: &[casm::Instruction]) -> Self {
        let mut locals: HashMap<String, InferredType> = HashMap::new();
        let mut stack: Vec<InferredType> = Vec::new();

        for instr in body {
            match instr.op.as_str() {
                "push_int" => stack.push(InferredType::Int),
                "push_float" => stack.push(InferredType::Float),
                "push_bool" | "push_null" | "push_str" => stack.push(InferredType::Dynamic),
                "pop" => { stack.pop(); }
                "dup" => {
                    let t = stack.last().cloned().unwrap_or(InferredType::Dynamic);
                    stack.push(t);
                }
                "store" => {
                    let name = instr.args.get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let t = stack.pop().unwrap_or(InferredType::Dynamic);
                    // If variable was already bound to a different type → Dynamic
                    let entry = locals.entry(name.to_string()).or_insert(t.clone());
                    if *entry != t {
                        *entry = InferredType::Dynamic;
                    }
                }
                "load" => {
                    let name = instr.args.get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let t = locals.get(name).cloned().unwrap_or(InferredType::Dynamic);
                    stack.push(t);
                }
                "add" | "sub" | "mul" | "div" | "mod" => {
                    let b = stack.pop().unwrap_or(InferredType::Dynamic);
                    let a = stack.pop().unwrap_or(InferredType::Dynamic);
                    let result = if a == b && a != InferredType::Dynamic { a } else { InferredType::Dynamic };
                    stack.push(result);
                }
                "neg" => {
                    let a = stack.pop().unwrap_or(InferredType::Dynamic);
                    stack.push(a);
                }
                "eq" | "ne" | "lt" | "gt" | "le" | "ge" => {
                    stack.pop();
                    stack.pop();
                    stack.push(InferredType::Dynamic); // boolean result, but we model as Dynamic
                }
                "ret" => { stack.clear(); }
                // conservative: anything else → clear stack type info
                _ => {
                    for t in stack.iter_mut() { *t = InferredType::Dynamic; }
                }
            }
        }
        Self { locals }
    }

    pub fn ctype_for(&self, name: &str) -> &'static str {
        match self.locals.get(name) {
            Some(InferredType::Int)   => "int64_t",
            Some(InferredType::Float) => "double",
            _                         => "CrushValue",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use casm::Instruction;
    use serde_json::json;

    fn instr(op: &str, args: serde_json::Value) -> Instruction {
        Instruction { op: op.into(), lang: None, meta: None, args }
    }

    #[test]
    fn test_int_inference() {
        let body = vec![
            instr("push_int", json!({"value": 10})),
            instr("store", json!({"name": "x"})),
            instr("push_int", json!({"value": 20})),
            instr("store", json!({"name": "y"})),
            instr("load",  json!({"name": "x"})),
            instr("load",  json!({"name": "y"})),
            instr("add",   json!({})),
            instr("store", json!({"name": "z"})),
        ];
        let tm = TypeMap::infer(&body);
        assert_eq!(tm.ctype_for("x"), "int64_t");
        assert_eq!(tm.ctype_for("y"), "int64_t");
        assert_eq!(tm.ctype_for("z"), "int64_t");
    }

    #[test]
    fn test_float_inference() {
        let body = vec![
            instr("push_float", json!({"value": 1.5})),
            instr("push_float", json!({"value": 2.5})),
            instr("add", json!({})),
            instr("store", json!({"name": "sum"})),
        ];
        let tm = TypeMap::infer(&body);
        assert_eq!(tm.ctype_for("sum"), "double");
    }

    #[test]
    fn test_dynamic_on_mixed() {
        let body = vec![
            instr("push_int",   json!({"value": 1})),
            instr("push_float", json!({"value": 1.0})),
            instr("add", json!({})),
            instr("store", json!({"name": "mixed"})),
        ];
        let tm = TypeMap::infer(&body);
        assert_eq!(tm.ctype_for("mixed"), "CrushValue");
    }
}
