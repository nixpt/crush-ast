//! Python AST expression → CAST expression lowering.

use py_ast::Ranged;
use crush_walker_core::LowerCtx;

use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::atomic::{AtomicUsize, Ordering};

use crush_cast::{CastType, Expression, Statement};
use rustpython_ast as py_ast;

/// Monotonic counter for comprehension temp-array names (`__comp_0`, `__comp_1`, ...).
/// Crush's VM has no lexical block scoping — variables live in one flat
/// per-function namespace (see the `__arr_N`/`__i_N` temps the compiler
/// already generates for desugared `for` loops in `crush-frontend/compiler.rs`)
/// — so uniqueness only needs to hold within one lowering pass, not
/// structurally nest.
static COMP_TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn fresh_comp_temp() -> String {
    let n = COMP_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("__comp_{n}")
}

/// Lower a Python AST expression to a CAST expression.
pub fn lower_expr(expr: &py_ast::Expr, ctx: &LowerCtx<'_>) -> anyhow::Result<Expression> {
    let offset = u32::from(expr.start()) as usize;
    let meta = ctx.meta_at(offset);
    match expr {
        py_ast::Expr::BoolOp(py_ast::ExprBoolOp { op, values, .. }) => {
            let mut iter = values.iter();
            let first = iter
                .next()
                .ok_or_else(|| anyhow::anyhow!("empty bool op"))?;
            let mut result = lower_expr(first, ctx)?;
            for val in iter {
                let right = lower_expr(val, ctx)?;
                let operator = match op {
                    py_ast::BoolOp::And => "and",
                    py_ast::BoolOp::Or => "or",
                };
                result = Expression::BinaryOp {
                    operator: operator.to_string(),
                    left: Box::new(result),
                    right: Box::new(right),
                    meta: meta.clone(),
                };
            }
            Ok(result)
        }
        py_ast::Expr::NamedExpr(py_ast::ExprNamedExpr { target, value, .. }) => {
            let value_expr = lower_expr(value, ctx)?;
            let name = name_from_expr(target)?;
            Ok(Expression::Call {
                function: "__crush_assign__".to_string(),
                args: vec![
                    value_expr,
                    Expression::Var {
                        name,
                        meta: meta.clone(),
                    },
                ],
                meta,
            })
        }
        py_ast::Expr::BinOp(py_ast::ExprBinOp {
            left, op, right, ..
        }) => {
            let left = lower_expr(left, ctx)?;
            let right = lower_expr(right, ctx)?;
            let operator = match op {
                py_ast::Operator::Add => "+",
                py_ast::Operator::Sub => "-",
                py_ast::Operator::Mult => "*",
                py_ast::Operator::Div => "/",
                py_ast::Operator::Mod => "%",
                py_ast::Operator::Pow => "**",
                py_ast::Operator::LShift => "<<",
                py_ast::Operator::RShift => ">>",
                py_ast::Operator::BitOr => "|",
                py_ast::Operator::BitXor => "^",
                py_ast::Operator::BitAnd => "&",
                py_ast::Operator::FloorDiv => "//",
                py_ast::Operator::MatMult => "@",
            };
            Ok(Expression::BinaryOp {
                operator: operator.to_string(),
                left: Box::new(left),
                right: Box::new(right),
                meta,
            })
        }
        py_ast::Expr::UnaryOp(py_ast::ExprUnaryOp { op, operand, .. }) => {
            let operand = lower_expr(operand, ctx)?;
            let operator = match op {
                py_ast::UnaryOp::USub => "-",
                py_ast::UnaryOp::UAdd => "+",
                py_ast::UnaryOp::Not => "not",
                py_ast::UnaryOp::Invert => "~",
            };
            Ok(Expression::UnaryOp {
                operator: operator.to_string(),
                operand: Box::new(operand),
                meta,
            })
        }
        py_ast::Expr::Lambda { .. } => {
            anyhow::bail!("lambda expressions not yet supported")
        }
        py_ast::Expr::IfExp(py_ast::ExprIfExp {
            test, body, orelse, ..
        }) => {
            let test = lower_expr(test, ctx)?;
            let body = lower_expr(body, ctx)?;
            let orelse = lower_expr(orelse, ctx)?;
            Ok(Expression::Call {
                function: "__crush_ifexpr__".to_string(),
                args: vec![test, body, orelse],
                meta,
            })
        }
        py_ast::Expr::Dict(py_ast::ExprDict { keys, values, .. }) => {
            let mut properties = Vec::new();
            for (k, v) in keys.iter().zip(values.iter()) {
                let key = match k {
                    Some(k) => constant_to_string(k)?,
                    None => anyhow::bail!("dict splat (**expr) not yet supported"),
                };
                let val = lower_expr(v, ctx)?;
                properties.push((key, val));
            }
            Ok(Expression::ObjectLiteral { properties, meta })
        }
        py_ast::Expr::Set { .. } => anyhow::bail!("set literals not yet supported"),
        py_ast::Expr::ListComp { .. } | py_ast::Expr::SetComp { .. } => {
            // These *are* lowerable (see `lower_expr_hoist` / `lower_list_or_set_comprehension`
            // below) — but only from a position that can absorb the hoisted
            // init+loop statements a comprehension needs. `lower_expr` itself
            // returns a bare `Expression` with nowhere to put those, so a
            // comprehension reached through plain `lower_expr` means it showed
            // up nested inside another expression (a binary op, an `if`
            // condition, another comprehension's `elt`, ...) that `lower_stmt`
            // doesn't hoist through. Bail loud rather than silently dropping it.
            anyhow::bail!(
                "comprehension used in a nested expression position not yet supported \
                 (only supported directly as an assignment RHS, a `return` value, or a \
                 call argument)"
            )
        }
        py_ast::Expr::DictComp { .. } => {
            // Confirmed VM gap, not just a missing lowering: crush-vm's SET_FIELD
            // opcode bakes the field name in as a compile-time constant
            // (`crush-vm/src/scheduler.rs` SET_FIELD reads it from `program.consts`
            // by a compile-time index) — there is no "insert at a runtime-computed
            // key" primitive for `Value::Map` anywhere in the VM today. A real
            // `{k: v for ...}` needs one. Filed via `dejavue plan`; not attempted
            // here per the "don't build new opcodes for this ticket" scope.
            anyhow::bail!(
                "dict comprehensions not yet supported — crush-vm has no dynamic-key \
                 map-insert primitive (SET_FIELD requires a compile-time-constant field \
                 name); needs a new opcode, out of scope for this ticket"
            )
        }
        py_ast::Expr::GeneratorExp { .. } => {
            anyhow::bail!(
                "generator expressions not yet supported — they desugar to a real \
                 generator function, which needs VM-level suspend/resume machinery \
                 (tracked separately in docs/design/python-lowering-coverage.md, not \
                 this ticket)"
            )
        }
        py_ast::Expr::Await(py_ast::ExprAwait { value, .. }) => {
            let expr = lower_expr(value, ctx)?;
            Ok(Expression::Await {
                expression: Box::new(expr),
                meta,
            })
        }
        py_ast::Expr::Yield { .. } | py_ast::Expr::YieldFrom { .. } => {
            anyhow::bail!("generators not yet supported")
        }
        py_ast::Expr::Compare(py_ast::ExprCompare {
            left,
            ops,
            comparators,
            ..
        }) => {
            let left = lower_expr(left, ctx)?;
            let op = &ops[0];
            let right = lower_expr(&comparators[0], ctx)?;
            match op {
                py_ast::CmpOp::Eq => Ok(Expression::BinaryOp { operator: "==".to_string(), left: Box::new(left), right: Box::new(right), meta }),
                py_ast::CmpOp::NotEq => Ok(Expression::BinaryOp { operator: "!=".to_string(), left: Box::new(left), right: Box::new(right), meta }),
                py_ast::CmpOp::Lt => Ok(Expression::BinaryOp { operator: "<".to_string(), left: Box::new(left), right: Box::new(right), meta }),
                py_ast::CmpOp::LtE => Ok(Expression::BinaryOp { operator: "<=".to_string(), left: Box::new(left), right: Box::new(right), meta }),
                py_ast::CmpOp::Gt => Ok(Expression::BinaryOp { operator: ">".to_string(), left: Box::new(left), right: Box::new(right), meta }),
                py_ast::CmpOp::GtE => Ok(Expression::BinaryOp { operator: ">=".to_string(), left: Box::new(left), right: Box::new(right), meta }),
                py_ast::CmpOp::In => Ok(Expression::Call {
                    function: "__crush_contains__".to_string(),
                    args: vec![right, left],
                    meta,
                }),
                py_ast::CmpOp::NotIn => Ok(Expression::UnaryOp {
                    operator: "not".to_string(),
                    operand: Box::new(Expression::Call {
                        function: "__crush_contains__".to_string(),
                        args: vec![right, left],
                        meta: meta.clone(),
                    }),
                    meta,
                }),
                py_ast::CmpOp::Is => Ok(Expression::Call {
                    function: "__crush_is__".to_string(),
                    args: vec![left, right],
                    meta,
                }),
                py_ast::CmpOp::IsNot => Ok(Expression::UnaryOp {
                    operator: "not".to_string(),
                    operand: Box::new(Expression::Call {
                        function: "__crush_is__".to_string(),
                        args: vec![left, right],
                        meta: meta.clone(),
                    }),
                    meta,
                }),
            }
        }
        py_ast::Expr::Call(py_ast::ExprCall {
            func,
            args,
            keywords,
            ..
        }) => {
            let lowered_args: Vec<Expression> = args
                .iter()
                .map(|a| lower_expr(a, ctx))
                .collect::<Result<Vec<_>, _>>()?;
            lower_call(func, lowered_args, keywords, meta)
        }
        py_ast::Expr::Constant(py_ast::ExprConstant { value, .. }) => lower_constant(value, meta),
        py_ast::Expr::Attribute(py_ast::ExprAttribute { value, attr, .. }) => {
            let target = lower_expr(value, ctx)?;
            Ok(Expression::GetField {
                target: Box::new(target),
                field: attr.to_string(),
                meta,
            })
        }
        py_ast::Expr::Subscript(py_ast::ExprSubscript { value, slice, .. }) => {
            let target = lower_expr(value, ctx)?;
            // If the subscript is a slice (arr[0:2]), emit __crush_slice__
            if let py_ast::Expr::Slice(py_ast::ExprSlice { lower, upper, step, .. }) = slice.as_ref() {
                let start = match lower {
                    Some(e) => lower_expr(e, ctx)?,
                    None => Expression::NullLiteral { meta: meta.clone() },
                };
                let end = match upper {
                    Some(e) => lower_expr(e, ctx)?,
                    None => Expression::NullLiteral { meta: meta.clone() },
                };
                let st = match step {
                    Some(e) => lower_expr(e, ctx)?,
                    None => Expression::NullLiteral { meta: meta.clone() },
                };
                Ok(Expression::Call {
                    function: "__crush_slice__".to_string(),
                    args: vec![target, start, end, st],
                    meta,
                })
            } else {
                let index = lower_expr(slice, ctx)?;
                Ok(Expression::Index {
                    target: Box::new(target),
                    index: Box::new(index),
                    meta,
                })
            }
        }
        py_ast::Expr::Starred { .. } => anyhow::bail!("starred expressions not yet supported"),
        py_ast::Expr::Name(py_ast::ExprName { id, .. }) => Ok(Expression::Var {
            name: id.to_string(),
            meta,
        }),
        py_ast::Expr::List(py_ast::ExprList { elts, .. }) => {
            let elements: Vec<Expression> = elts
                .iter()
                .map(|e| lower_expr(e, ctx))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Expression::ArrayLiteral { elements, meta })
        }
        py_ast::Expr::Tuple(py_ast::ExprTuple { elts, .. }) => {
            let elements: Vec<Expression> = elts
                .iter()
                .map(|e| lower_expr(e, ctx))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Expression::ArrayLiteral { elements, meta })
        }
        py_ast::Expr::Slice(py_ast::ExprSlice { lower, upper, step, .. }) => {
            let start = match lower {
                Some(e) => lower_expr(e, ctx)?,
                None => Expression::NullLiteral { meta: meta.clone() },
            };
            let end = match upper {
                Some(e) => lower_expr(e, ctx)?,
                None => Expression::NullLiteral { meta: meta.clone() },
            };
            let step = match step {
                Some(e) => lower_expr(e, ctx)?,
                None => Expression::NullLiteral { meta: meta.clone() },
            };
            Ok(Expression::Call {
                function: "__crush_slice__".to_string(),
                args: vec![start, end, step],
                meta,
            })
        }
        py_ast::Expr::JoinedStr(py_ast::ExprJoinedStr { values, .. }) => {
            let mut parts: Vec<String> = Vec::new();
            for val in values {
                match val {
                    py_ast::Expr::Constant(py_ast::ExprConstant { value, .. }) => {
                        parts.push(match value {
                            py_ast::Constant::Str(s) => s.clone(),
                            _ => format!("{:?}", value),
                        });
                    }
                    py_ast::Expr::FormattedValue(..) => {
                        anyhow::bail!("f-string interpolation not yet supported")
                    }
                    _ => anyhow::bail!("unexpected f-string part"),
                }
            }
            Ok(Expression::StringLiteral {
                value: parts.concat(),
                meta,
            })
        }
        py_ast::Expr::FormattedValue(..) => {
            anyhow::bail!("formatted values not yet supported")
        }
    }
}

fn lower_call(
    func: &py_ast::Expr,
    lowered_args: Vec<Expression>,
    _keywords: &[py_ast::Keyword],
    meta: HashMap<String, serde_json::Value>,
) -> anyhow::Result<Expression> {
    let func_name = match func {
        py_ast::Expr::Name(py_ast::ExprName { id, .. }) => id.to_string(),
        py_ast::Expr::Attribute(py_ast::ExprAttribute { value, attr, .. }) => {
            let obj = match value.as_ref() {
                py_ast::Expr::Name(py_ast::ExprName { id, .. }) => id.to_string(),
                _ => return Err(anyhow::anyhow!("complex method calls not yet supported")),
            };
            format!("{}.{}", obj, attr)
        }
        _ => return Err(anyhow::anyhow!("complex function calls not yet supported")),
    };

    match func_name.as_str() {
        "print" => Ok(Expression::CapabilityCall {
            name: "io.print".to_string(),
            args: lowered_args,
            meta,
        }),
        "len" => Ok(Expression::Call {
            function: "len".to_string(),
            args: lowered_args,
            meta,
        }),
        "int" | "float" | "str" | "bool" | "list" | "dict" => Ok(Expression::Call {
            function: func_name,
            args: lowered_args,
            meta,
        }),
        "range" => Ok(Expression::Call {
            function: "make_range".to_string(),
            args: lowered_args,
            meta,
        }),
        _ => Ok(Expression::Call {
            function: func_name,
            args: lowered_args,
            meta,
        }),
    }
}

fn lower_constant(
    value: &py_ast::Constant,
    meta: HashMap<String, serde_json::Value>,
) -> anyhow::Result<Expression> {
    match value {
        py_ast::Constant::None => Ok(Expression::NullLiteral { meta }),
        py_ast::Constant::Bool(b) => Ok(Expression::BoolLiteral { value: *b, meta }),
        py_ast::Constant::Int(i) => {
            let val: i64 = match i.try_into() {
                Ok(v) => v,
                Err(_) => anyhow::bail!("integer overflow: {}", i),
            };
            Ok(Expression::IntLiteral { value: val, meta })
        }
        py_ast::Constant::Float(f) => Ok(Expression::FloatLiteral { value: *f, meta }),
        py_ast::Constant::Str(s) => Ok(Expression::StringLiteral {
            value: s.clone(),
            meta,
        }),
        py_ast::Constant::Bytes(_) => anyhow::bail!("bytes literals not yet supported"),
        py_ast::Constant::Complex { .. } => anyhow::bail!("complex numbers not yet supported"),
        py_ast::Constant::Ellipsis => anyhow::bail!("ellipsis literal not yet supported"),
        py_ast::Constant::Tuple(t) => {
            let elements: Vec<Expression> = t
                .iter()
                .map(|e| match e {
                    py_ast::Constant::None => Ok(Expression::NullLiteral { meta: meta.clone() }),
                    py_ast::Constant::Bool(b) => Ok(Expression::BoolLiteral {
                        value: *b,
                        meta: meta.clone(),
                    }),
                    py_ast::Constant::Int(i) => {
                        let val: i64 = i.try_into().map_err(|_| anyhow::anyhow!("int overflow"))?;
                        Ok(Expression::IntLiteral {
                            value: val,
                            meta: meta.clone(),
                        })
                    }
                    py_ast::Constant::Float(f) => Ok(Expression::FloatLiteral {
                        value: *f,
                        meta: meta.clone(),
                    }),
                    py_ast::Constant::Str(s) => Ok(Expression::StringLiteral {
                        value: s.clone(),
                        meta: meta.clone(),
                    }),
                    _ => anyhow::bail!("nested constant tuples not yet supported"),
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Expression::ArrayLiteral { elements, meta })
        }
    }
}

fn constant_to_string(expr: &py_ast::Expr) -> anyhow::Result<String> {
    match expr {
        py_ast::Expr::Constant(py_ast::ExprConstant { value, .. }) => match value {
            py_ast::Constant::Str(s) => Ok(s.clone()),
            py_ast::Constant::Int(i) => Ok(format!("{}", i)),
            py_ast::Constant::Bool(b) => Ok(if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }),
            py_ast::Constant::None => Ok("null".to_string()),
            py_ast::Constant::Float(f) => Ok(format!("{}", f)),
            py_ast::Constant::Ellipsis => anyhow::bail!("ellipsis as dict key"),
            _ => anyhow::bail!("unsupported constant as dict key"),
        },
        _ => anyhow::bail!("dict keys must be constant expressions"),
    }
}

fn name_from_expr(expr: &py_ast::Expr) -> anyhow::Result<String> {
    match expr {
        py_ast::Expr::Name(py_ast::ExprName { id, .. }) => Ok(id.to_string()),
        _ => anyhow::bail!("expected identifier, got {:?}", expr),
    }
}

// ── Comprehensions ──────────────────────────────────────────────────────────
//
// `crush-cast` has no comprehension-specific node (and doesn't need one —
// see docs/design/python-lowering-coverage.md §3b: real Python compiles
// these to an ordinary `for` loop + append, no comprehension-specific
// bytecode exists anywhere). So a list/set comprehension desugars, right
// here in the lowerer, to:
//
//     let __comp_N = []
//     for <target> in <iter> {
//         if <ifs[0]> { if <ifs[1]> { ... __comp_N.append(<elt>) ... } }
//     }
//
// (nested `for`s for multiple `generators`, nested `if`s for multiple `ifs`
// on one generator) and the comprehension *expression* evaluates to
// `Var(__comp_N)`. `.append()` already works end-to-end — it's the exact
// path `arr.append(x)` compiles through (crush-frontend/compiler.rs's
// `Expression::Call` "obj.method(args)" split), proven by the existing
// `test_list_ops` in sdk.rs.

/// Lower `[elt for target in iter if cond ...]` / `{elt for ...}` (set — no
/// native Set type in this VM, so it desugars identically to a list; callers
/// lose Python's dedup-on-insert semantics, which is a known, accepted
/// narrowing rather than a silent one).
fn lower_list_or_set_comprehension(
    elt: &py_ast::Expr,
    generators: &[py_ast::Comprehension],
    ctx: &LowerCtx<'_>,
    meta: HashMap<String, serde_json::Value>,
) -> anyhow::Result<(Vec<Statement>, Expression)> {
    if generators.is_empty() {
        anyhow::bail!("comprehension with no `for` clause");
    }
    let tmp = fresh_comp_temp();
    let init = Statement::VarDecl {
        name: tmp.clone(),
        value: Expression::ArrayLiteral {
            elements: vec![],
            meta: meta.clone(),
        },
        type_hint: CastType::Any,
        meta: meta.clone(),
    };
    let elt_lowered = lower_expr(elt, ctx)?;
    let push_stmt = Statement::ExprStmt {
        expr: Expression::Call {
            function: format!("{tmp}.append"),
            args: vec![elt_lowered],
            meta: meta.clone(),
        },
        meta: meta.clone(),
    };
    let loop_stmt = build_comprehension_loop(generators, 0, vec![push_stmt], ctx, &meta)?;
    Ok((
        vec![init, loop_stmt],
        Expression::Var {
            name: tmp,
            meta,
        },
    ))
}

/// Recursively build the (possibly nested, for multiple `for` clauses)
/// `for`/`if` chain that ends in `innermost` (the append statement).
fn build_comprehension_loop(
    generators: &[py_ast::Comprehension],
    idx: usize,
    innermost: Vec<Statement>,
    ctx: &LowerCtx<'_>,
    meta: &HashMap<String, serde_json::Value>,
) -> anyhow::Result<Statement> {
    // `gen` is a reserved keyword as of the 2024 edition (reserved for a
    // future generator-block feature) — this crate is `edition = "2024"`,
    // so the obvious variable name doesn't compile. Named `generator` instead.
    let generator = &generators[idx];
    if generator.is_async {
        anyhow::bail!("async comprehensions (`async for` inside a comprehension) not yet supported");
    }
    let var = match &generator.target {
        py_ast::Expr::Name(py_ast::ExprName { id, .. }) => id.to_string(),
        _ => anyhow::bail!(
            "comprehension target must be a simple name — tuple-unpacking targets \
             (`for k, v in items`) not yet supported"
        ),
    };
    let iterable = lower_expr(&generator.iter, ctx)?;
    let mut body = if idx + 1 < generators.len() {
        vec![build_comprehension_loop(generators, idx + 1, innermost, ctx, meta)?]
    } else {
        innermost
    };
    for cond in generator.ifs.iter().rev() {
        let c = lower_expr(cond, ctx)?;
        body = vec![Statement::If {
            condition: c,
            then_body: body,
            else_body: None,
            meta: meta.clone(),
        }];
    }
    Ok(Statement::For {
        variable: var,
        iterable: Box::new(iterable),
        body,
        meta: meta.clone(),
    })
}

/// Lower an expression that might itself be a comprehension, or contain one
/// as a call argument. Comprehensions need to run a loop *before* they
/// produce a value — something a plain `lower_expr(...) -> Expression` has
/// no way to express, since CAST has no block-expression/IIFE construct
/// (unlike `NamedExpr`'s `__crush_assign__` trick, a single value swap can't
/// cover "run N statements, then use a value").
///
/// This is the "smuggle statements into expression position" mechanism:
/// it returns the statements that must run *before* the expression they're
/// paired with, and callers (`lower_stmt.rs`'s Assign/Expr/Return arms) are
/// responsible for splicing them back into the statement stream — via a
/// `Statement::If(true) { hoisted..., actual }` block, since that's an
/// existing CAST primitive that already compiles and runs (Crush's VM has
/// no lexical block scoping, so this only groups statements — it doesn't
/// need to be a *scope*).
///
/// Recurses through `Call` arguments so comprehensions nested arbitrarily
/// deep inside call arguments still hoist (`foo(bar([x for x in y]))`).
/// Everything else falls back to plain `lower_expr`, so a comprehension
/// nested inside a binary op, an `if` condition, or another comprehension's
/// own `elt`/`iter` still bails loud there — deliberately: see the
/// `ListComp`/`SetComp` bail message in `lower_expr` above.
pub(crate) fn lower_expr_hoist(
    expr: &py_ast::Expr,
    ctx: &LowerCtx<'_>,
) -> anyhow::Result<(Vec<Statement>, Expression)> {
    let offset = u32::from(expr.start()) as usize;
    let meta = ctx.meta_at(offset);
    match expr {
        py_ast::Expr::ListComp(py_ast::ExprListComp { elt, generators, .. }) => {
            lower_list_or_set_comprehension(elt, generators, ctx, meta)
        }
        py_ast::Expr::SetComp(py_ast::ExprSetComp { elt, generators, .. }) => {
            lower_list_or_set_comprehension(elt, generators, ctx, meta)
        }
        py_ast::Expr::Call(py_ast::ExprCall {
            func,
            args,
            keywords,
            ..
        }) => {
            let mut hoisted = Vec::new();
            let mut lowered_args = Vec::new();
            for a in args {
                let (h, e) = lower_expr_hoist(a, ctx)?;
                hoisted.extend(h);
                lowered_args.push(e);
            }
            let call_expr = lower_call(func, lowered_args, keywords, meta)?;
            Ok((hoisted, call_expr))
        }
        _ => Ok((Vec::new(), lower_expr(expr, ctx)?)),
    }
}

#[cfg(test)]
mod comprehension_lowering_tests {
    use super::*;

    fn ctx() -> LowerCtx<'static> {
        LowerCtx::new("", "<test>", "python")
    }

    #[test]
    fn list_comp_desugars_to_init_plus_for_loop() {
        let src = "[i * i for i in range(5)]";
        let expr = crate::parser::parse_expression(src).unwrap();
        let c = ctx();
        let (hoisted, result) = lower_expr_hoist(&expr, &c).expect("comprehension should lower");

        assert_eq!(hoisted.len(), 2, "expected [init, for-loop], got {hoisted:?}");
        match &hoisted[0] {
            Statement::VarDecl { value: Expression::ArrayLiteral { elements, .. }, .. } => {
                assert!(elements.is_empty());
            }
            other => panic!("expected VarDecl(empty array) init, got {other:?}"),
        }
        match &hoisted[1] {
            Statement::For { variable, body, .. } => {
                assert_eq!(variable, "i");
                assert_eq!(body.len(), 1);
                match &body[0] {
                    Statement::ExprStmt { expr: Expression::Call { function, args, .. }, .. } => {
                        assert!(function.ends_with(".append"));
                        assert_eq!(args.len(), 1);
                    }
                    other => panic!("expected append call, got {other:?}"),
                }
            }
            other => panic!("expected For loop, got {other:?}"),
        }
        match result {
            Expression::Var { name, .. } => assert!(name.starts_with("__comp_")),
            other => panic!("expected comprehension to evaluate to the temp var, got {other:?}"),
        }
    }

    #[test]
    fn list_comp_with_if_clause_wraps_append_in_if() {
        let src = "[i for i in range(10) if i % 2 == 0]";
        let expr = crate::parser::parse_expression(src).unwrap();
        let c = ctx();
        let (hoisted, _) = lower_expr_hoist(&expr, &c).expect("comprehension should lower");
        match &hoisted[1] {
            Statement::For { body, .. } => match &body[0] {
                Statement::If { then_body, else_body, .. } => {
                    assert!(else_body.is_none());
                    assert_eq!(then_body.len(), 1);
                }
                other => panic!("expected `if` wrapping the append, got {other:?}"),
            },
            other => panic!("expected For loop, got {other:?}"),
        }
    }

    #[test]
    fn dict_comp_bails_loud_with_vm_gap_explanation() {
        let src = "{k: k for k in range(3)}";
        let expr = crate::parser::parse_expression(src).unwrap();
        let c = ctx();
        let err = lower_expr(&expr, &c).unwrap_err();
        assert!(
            err.to_string().contains("dynamic-key"),
            "expected the dict-comp bail to explain the SET_FIELD gap, got: {err}"
        );
    }

    #[test]
    fn generator_expression_bails_loud() {
        let src = "(i for i in range(3))";
        let expr = crate::parser::parse_expression(src).unwrap();
        let c = ctx();
        let err = lower_expr(&expr, &c).unwrap_err();
        assert!(err.to_string().contains("generator"));
    }
}
