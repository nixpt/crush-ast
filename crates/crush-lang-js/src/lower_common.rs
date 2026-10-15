//! CAST helpers shared by the swc and boa lowerings (parser-independent).

#![cfg_attr(not(any(feature = "swc-backend", feature = "js-boa")), allow(dead_code))]

use std::collections::HashMap;

use crush_cast::{CastType, Expression, Statement};

/// Make every `continue` that belongs to the enclosing loop run `prefix` first
/// (a C-style `for`'s update, a `do-while`'s test). Nested loops and functions
/// own their own `continue`s and are left alone.
pub(crate) fn rewrite_continue(stmts: Vec<Statement>, prefix: &[Statement]) -> Vec<Statement> {
    if prefix.is_empty() {
        return stmts;
    }
    let mut out = Vec::with_capacity(stmts.len());
    for s in stmts {
        match s {
            Statement::Continue { meta } => {
                out.extend(prefix.iter().cloned());
                out.push(Statement::Continue { meta });
            }
            Statement::If { condition, then_body, else_body, meta } => out.push(Statement::If {
                condition,
                then_body: rewrite_continue(then_body, prefix),
                else_body: else_body.map(|e| rewrite_continue(e, prefix)),
                meta,
            }),
            Statement::TryCatch { body, error_var, handler, meta } => out.push(Statement::TryCatch {
                body: rewrite_continue(body, prefix),
                error_var,
                handler: rewrite_continue(handler, prefix),
                meta,
            }),
            other => out.push(other),
        }
    }
    out
}

/// `for (variable in iterable) body` over a value whose type isn't known
/// statically:
///   let __forin_N = iterable;
///   let __forin_ty_N = type_of(__forin_N);
///   if (__forin_ty_N == "array" || … "str" | "string" …) throw "…";
///   for variable in __forin_N { body }
/// grouped in one `if (true) { … }`. The type names cover every engine
/// (CVM1 says "str", FastVM/JIT "string"); objects and maps iterate their keys
/// as before. AOT backends without `type_of` refuse it at compile time.
pub(crate) fn for_in_guarded(
    variable: String,
    iterable: Expression,
    body: Vec<Statement>,
    id: u32,
    sm: HashMap<String, serde_json::Value>,
) -> Statement {
    let obj = format!("__forin_{id}");
    let ty = format!("__forin_ty_{id}");
    let var = |name: &str| Expression::Var { name: name.to_string(), meta: sm.clone() };
    let is_sequence = ["array", "str", "string", "list", "vector", "tuple"]
        .iter()
        .map(|t| Expression::BinaryOp {
            operator: "==".to_string(),
            left: Box::new(var(&ty)),
            right: Box::new(Expression::StringLiteral { value: t.to_string(), meta: sm.clone() }),
            meta: sm.clone(),
        })
        .reduce(|a, b| Expression::BinaryOp {
            operator: "||".to_string(),
            left: Box::new(a),
            right: Box::new(b),
            meta: sm.clone(),
        })
        .expect("non-empty");
    Statement::If {
        condition: Expression::BoolLiteral { value: true, meta: sm.clone() },
        then_body: vec![
            Statement::VarDecl { name: obj.clone(), value: iterable, type_hint: CastType::Any, meta: sm.clone() },
            Statement::VarDecl {
                name: ty.clone(),
                value: Expression::CapabilityCall { name: "type_of".to_string(), args: vec![var(&obj)], meta: sm.clone() },
                type_hint: CastType::Any,
                meta: sm.clone(),
            },
            Statement::If {
                condition: is_sequence,
                then_body: vec![Statement::Throw {
                    value: Expression::StringLiteral {
                        value: "`for-in` over an array/string is not supported by the JS walker yet".to_string(),
                        meta: sm.clone(),
                    },
                    meta: sm.clone(),
                }],
                else_body: None,
                meta: sm.clone(),
            },
            Statement::For { variable, iterable: Box::new(var(&obj)), body, meta: sm.clone() },
        ],
        else_body: None,
        meta: sm,
    }
}

/// Does `stmts` contain a `continue` that belongs to the enclosing loop
/// (i.e. not inside a nested loop or function)?
pub(crate) fn contains_loop_continue(stmts: &[Statement]) -> bool {
    stmts.iter().any(|s| match s {
        Statement::Continue { .. } => true,
        Statement::If { then_body, else_body, .. } => {
            contains_loop_continue(then_body) || else_body.as_deref().is_some_and(contains_loop_continue)
        }
        Statement::TryCatch { body, handler, .. } => contains_loop_continue(body) || contains_loop_continue(handler),
        _ => false,
    })
}

/// JS `/` is real division (`7 / 2` is 3.5; crush's `/` on two ints
/// truncates). An exact quotient of two ints stays an int (`4 / 2` is `2`,
/// usable as an array index and printed `2` even inside string
/// concatenation) — JS has one number type, so that is safe. Division by
/// zero is a loud error rather than Infinity.
pub(crate) fn js_div(left: Expression, right: Expression, meta: HashMap<String, serde_json::Value>) -> Expression {
    Expression::CapabilityCall { name: "js.div".to_string(), args: vec![left, right], meta }
}

/// A call to a `js.*` capability: JS semantics implemented once in crush-vm
/// (`js_core`) and mirrored by the AOT runtimes.
pub(crate) fn js_cap(name: &str, args: Vec<Expression>, meta: HashMap<String, serde_json::Value>) -> Expression {
    Expression::CapabilityCall { name: name.to_string(), args, meta }
}

pub(crate) fn length_of(target: Expression, meta: HashMap<String, serde_json::Value>) -> Expression {
    Expression::Call { function: "len".to_string(), args: vec![target], meta }
}
