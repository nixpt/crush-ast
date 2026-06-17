//! Python AST expression → CAST expression lowering.

use std::collections::HashMap;
use std::convert::TryInto;

use crush_cast::Expression;
use rustpython_ast as py_ast;

/// Lower a Python AST expression to a CAST expression.
pub fn lower_expr(expr: &py_ast::Expr) -> anyhow::Result<Expression> {
    let meta = HashMap::new();
    match expr {
        py_ast::Expr::BoolOp(py_ast::ExprBoolOp { op, values, .. }) => {
            let mut iter = values.iter();
            let first = iter.next().ok_or_else(|| anyhow::anyhow!("empty bool op"))?;
            let mut result = lower_expr(first)?;
            for val in iter {
                let right = lower_expr(val)?;
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
            let value_expr = lower_expr(value)?;
            let name = name_from_expr(target)?;
            Ok(Expression::Call {
                function: "__crush_assign__".to_string(),
                args: vec![value_expr, Expression::Var { name, meta: meta.clone() }],
                meta,
            })
        }
        py_ast::Expr::BinOp(py_ast::ExprBinOp { left, op, right, .. }) => {
            let left = lower_expr(left)?;
            let right = lower_expr(right)?;
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
            let operand = lower_expr(operand)?;
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
        py_ast::Expr::IfExp(py_ast::ExprIfExp { test, body, orelse, .. }) => {
            let test = lower_expr(test)?;
            let body = lower_expr(body)?;
            let orelse = lower_expr(orelse)?;
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
                let val = lower_expr(v)?;
                properties.push((key, val));
            }
            Ok(Expression::ObjectLiteral { properties, meta })
        }
        py_ast::Expr::Set { .. } => anyhow::bail!("set literals not yet supported"),
        py_ast::Expr::ListComp { .. } | py_ast::Expr::SetComp { .. }
        | py_ast::Expr::DictComp { .. } | py_ast::Expr::GeneratorExp { .. } => {
            anyhow::bail!("comprehensions not yet supported")
        }
        py_ast::Expr::Await { .. } => anyhow::bail!("async/await not yet supported"),
        py_ast::Expr::Yield { .. } | py_ast::Expr::YieldFrom { .. } => {
            anyhow::bail!("generators not yet supported")
        }
        py_ast::Expr::Compare(py_ast::ExprCompare { left, ops, comparators, .. }) => {
            let left = lower_expr(left)?;
            let op = &ops[0];
            let right = lower_expr(&comparators[0])?;
            let operator = match op {
                py_ast::CmpOp::Eq => "==",
                py_ast::CmpOp::NotEq => "!=",
                py_ast::CmpOp::Lt => "<",
                py_ast::CmpOp::LtE => "<=",
                py_ast::CmpOp::Gt => ">",
                py_ast::CmpOp::GtE => ">=",
                py_ast::CmpOp::In | py_ast::CmpOp::NotIn => {
                    anyhow::bail!("'in' operator not yet supported")
                }
                py_ast::CmpOp::Is | py_ast::CmpOp::IsNot => {
                    anyhow::bail!("'is' operator not yet supported")
                }
            };
            Ok(Expression::BinaryOp {
                operator: operator.to_string(),
                left: Box::new(left),
                right: Box::new(right),
                meta,
            })
        }
        py_ast::Expr::Call(py_ast::ExprCall { func, args, keywords, .. }) => {
            lower_call(func, args, keywords, meta)
        }
        py_ast::Expr::Constant(py_ast::ExprConstant { value, .. }) => {
            lower_constant(value, meta)
        }
        py_ast::Expr::Attribute(py_ast::ExprAttribute { value, attr, .. }) => {
            let target = lower_expr(value)?;
            Ok(Expression::GetField {
                target: Box::new(target),
                field: attr.to_string(),
                meta,
            })
        }
        py_ast::Expr::Subscript(py_ast::ExprSubscript { value, slice, .. }) => {
            let target = lower_expr(value)?;
            let index = lower_expr(slice)?;
            Ok(Expression::Index {
                target: Box::new(target),
                index: Box::new(index),
                meta,
            })
        }
        py_ast::Expr::Starred { .. } => anyhow::bail!("starred expressions not yet supported"),
        py_ast::Expr::Name(py_ast::ExprName { id, .. }) => {
            Ok(Expression::Var { name: id.to_string(), meta })
        }
        py_ast::Expr::List(py_ast::ExprList { elts, .. }) => {
            let elements: Vec<Expression> = elts.iter().map(|e| lower_expr(e)).collect::<Result<Vec<_>, _>>()?;
            Ok(Expression::ArrayLiteral { elements, meta })
        }
        py_ast::Expr::Tuple(py_ast::ExprTuple { elts, .. }) => {
            let elements: Vec<Expression> = elts.iter().map(|e| lower_expr(e)).collect::<Result<Vec<_>, _>>()?;
            Ok(Expression::ArrayLiteral { elements, meta })
        }
        py_ast::Expr::Slice { .. } => anyhow::bail!("slice expressions not yet supported"),
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
            Ok(Expression::StringLiteral { value: parts.concat(), meta })
        }
        py_ast::Expr::FormattedValue(..) => {
            anyhow::bail!("formatted values not yet supported")
        }
    }
}

fn lower_call(
    func: &py_ast::Expr,
    args: &[py_ast::Expr],
    _keywords: &[py_ast::Keyword],
    meta: HashMap<String, serde_json::Value>,
) -> anyhow::Result<Expression> {
    let lowered_args: Vec<Expression> = args.iter().map(|a| lower_expr(a)).collect::<Result<Vec<_>, _>>()?;

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
            name: "io.print".to_string(), args: lowered_args, meta,
        }),
        "len" => Ok(Expression::Call {
            function: "len".to_string(), args: lowered_args, meta,
        }),
        "int" | "float" | "str" | "bool" | "list" | "dict" => {
            Ok(Expression::Call { function: func_name, args: lowered_args, meta })
        }
        "range" => Ok(Expression::Call {
            function: "make_range".to_string(), args: lowered_args, meta,
        }),
        _ => Ok(Expression::Call { function: func_name, args: lowered_args, meta }),
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
            value: s.clone(), meta,
        }),
        py_ast::Constant::Bytes(_) => anyhow::bail!("bytes literals not yet supported"),
        py_ast::Constant::Complex { .. } => anyhow::bail!("complex numbers not yet supported"),
        py_ast::Constant::Ellipsis => anyhow::bail!("ellipsis literal not yet supported"),
        py_ast::Constant::Tuple(t) => {
            let elements: Vec<Expression> = t.iter().map(|e| {
                match e {
                    py_ast::Constant::None => Ok(Expression::NullLiteral { meta: meta.clone() }),
                    py_ast::Constant::Bool(b) => Ok(Expression::BoolLiteral { value: *b, meta: meta.clone() }),
                    py_ast::Constant::Int(i) => {
                        let val: i64 = i.try_into().map_err(|_| anyhow::anyhow!("int overflow"))?;
                        Ok(Expression::IntLiteral { value: val, meta: meta.clone() })
                    }
                    py_ast::Constant::Float(f) => Ok(Expression::FloatLiteral { value: *f, meta: meta.clone() }),
                    py_ast::Constant::Str(s) => Ok(Expression::StringLiteral { value: s.clone(), meta: meta.clone() }),
                    _ => anyhow::bail!("nested constant tuples not yet supported"),
                }
            }).collect::<Result<Vec<_>, _>>()?;
            Ok(Expression::ArrayLiteral { elements, meta })
        }
    }
}

fn constant_to_string(expr: &py_ast::Expr) -> anyhow::Result<String> {
    match expr {
        py_ast::Expr::Constant(py_ast::ExprConstant { value, .. }) => {
            match value {
                py_ast::Constant::Str(s) => Ok(s.clone()),
                py_ast::Constant::Int(i) => Ok(format!("{}", i)),
                py_ast::Constant::Bool(b) => Ok(if *b { "true".to_string() } else { "false".to_string() }),
                py_ast::Constant::None => Ok("null".to_string()),
                py_ast::Constant::Float(f) => Ok(format!("{}", f)),
                py_ast::Constant::Ellipsis => anyhow::bail!("ellipsis as dict key"),
                _ => anyhow::bail!("unsupported constant as dict key"),
            }
        }
        _ => anyhow::bail!("dict keys must be constant expressions"),
    }
}

fn name_from_expr(expr: &py_ast::Expr) -> anyhow::Result<String> {
    match expr {
        py_ast::Expr::Name(py_ast::ExprName { id, .. }) => Ok(id.to_string()),
        _ => anyhow::bail!("expected identifier, got {:?}", expr),
    }
}
