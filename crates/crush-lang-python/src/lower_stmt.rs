//! Python AST statement → CAST statement lowering.

use py_ast::Ranged;
use walker_core::LowerCtx;

use crush_cast::{CastType, Expression, ImportStatement, Statement};
use rustpython_ast as py_ast;

use crate::lower_expr::lower_expr;

/// Lower a Python AST statement to a CAST statement.
pub fn lower_stmt(stmt: &py_ast::Stmt, ctx: &LowerCtx<'_>) -> anyhow::Result<Statement> {
    let offset = u32::from(stmt.start()) as usize;
    let meta = ctx.meta_at(offset);
    match stmt {
        py_ast::Stmt::FunctionDef(py_ast::StmtFunctionDef {
            name, args, body, ..
        }) => {
            let params: Vec<(String, CastType)> = args
                .args
                .iter()
                .map(|a| (a.def.arg.to_string(), CastType::Any))
                .collect();
            let mut lowered_body = Vec::new();
            for s in body {
                lowered_body.push(lower_stmt(s, ctx)?);
            }
            Ok(Statement::FunctionDef {
                name: name.to_string(),
                params,
                body: lowered_body,
                meta,
            })
        }
        py_ast::Stmt::Return(py_ast::StmtReturn { value, .. }) => {
            let value = match value {
                Some(v) => Some(lower_expr(v, ctx)?),
                None => None,
            };
            Ok(Statement::Return { value, meta })
        }
        py_ast::Stmt::Assign(py_ast::StmtAssign { targets, value, .. }) => {
            let value = lower_expr(value, ctx)?;
            if targets.len() != 1 {
                anyhow::bail!("multi-target assignment not yet supported");
            }
            let target = &targets[0];
            match target {
                py_ast::Expr::Name(py_ast::ExprName { id, .. }) => Ok(Statement::VarDecl {
                    name: id.to_string(),
                    value,
                    type_hint: CastType::Any,
                    meta,
                }),
                py_ast::Expr::Attribute(py_ast::ExprAttribute {
                    value: obj, attr, ..
                }) => {
                    let target = lower_expr(obj, ctx)?;
                    Ok(Statement::SetField {
                        target,
                        field: attr.to_string(),
                        value,
                        meta,
                    })
                }
                py_ast::Expr::Subscript(py_ast::ExprSubscript {
                    value: obj, slice, ..
                }) => {
                    let target = lower_expr(obj, ctx)?;
                    let index = lower_expr(slice, ctx)?;
                    Ok(Statement::ExprStmt {
                        expr: Expression::Call {
                            function: "__crush_setindex__".to_string(),
                            args: vec![target, index, value],
                            meta: meta.clone(),
                        },
                        meta,
                    })
                }
                _ => anyhow::bail!("unsupported assignment target: {:?}", target),
            }
        }
        py_ast::Stmt::AugAssign(py_ast::StmtAugAssign {
            target, op, value, ..
        }) => {
            let target_name: String = match target.as_ref() {
                py_ast::Expr::Name(py_ast::ExprName { id, .. }) => id.to_string(),
                _ => anyhow::bail!("augmented assignment target must be a name"),
            };
            let val = lower_expr(value, ctx)?;
            Ok(Statement::VarDecl {
                name: target_name.to_string(),
                value: Expression::BinaryOp {
                    operator: match op {
                        py_ast::Operator::Add => "+",
                        py_ast::Operator::Sub => "-",
                        py_ast::Operator::Mult => "*",
                        py_ast::Operator::Div => "/",
                        _ => anyhow::bail!("unsupported augmented operator"),
                    }
                    .to_string(),
                    left: Box::new(Expression::Var {
                        name: target_name.to_string(),
                        meta: meta.clone(),
                    }),
                    right: Box::new(val),
                    meta: meta.clone(),
                },
                type_hint: CastType::Any,
                meta,
            })
        }
        py_ast::Stmt::Expr(py_ast::StmtExpr { value, .. }) => {
            let expr = lower_expr(value, ctx)?;
            Ok(Statement::ExprStmt { expr, meta })
        }
        py_ast::Stmt::If(py_ast::StmtIf {
            test, body, orelse, ..
        }) => {
            let condition = lower_expr(test, ctx)?;
            let then_body: Vec<Statement> = body
                .iter()
                .map(|s| lower_stmt(s, ctx))
                .collect::<Result<Vec<_>, _>>()?;
            let else_body: Option<Vec<Statement>> = if orelse.is_empty() {
                None
            } else {
                Some(
                    orelse
                        .iter()
                        .map(|s| lower_stmt(s, ctx))
                        .collect::<Result<Vec<_>, _>>()?,
                )
            };
            Ok(Statement::If {
                condition,
                then_body,
                else_body,
                meta,
            })
        }
        py_ast::Stmt::While(py_ast::StmtWhile { test, body, .. }) => {
            let condition = lower_expr(test, ctx)?;
            let body: Vec<Statement> = body
                .iter()
                .map(|s| lower_stmt(s, ctx))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Statement::While {
                condition: Box::new(condition),
                body,
                meta,
            })
        }
        py_ast::Stmt::For(py_ast::StmtFor {
            target, iter, body, ..
        }) => {
            let variable: String = match target.as_ref() {
                py_ast::Expr::Name(py_ast::ExprName { id, .. }) => id.to_string(),
                _ => anyhow::bail!("for loop target must be a name"),
            };
            let iterable = lower_expr(iter, ctx)?;
            let body: Vec<Statement> = body
                .iter()
                .map(|s| lower_stmt(s, ctx))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Statement::For {
                variable,
                iterable: Box::new(iterable),
                body,
                meta,
            })
        }
        py_ast::Stmt::Break { .. } => Ok(Statement::Break { meta }),
        py_ast::Stmt::Continue { .. } => Ok(Statement::Continue { meta }),
        py_ast::Stmt::Raise { .. } => anyhow::bail!("raise not yet supported"),
        py_ast::Stmt::Try { .. } => anyhow::bail!("try/except not yet supported"),
        py_ast::Stmt::Import(py_ast::StmtImport { names, .. }) => {
            if names.is_empty() {
                anyhow::bail!("empty import");
            }
            let name = &names[0];
            Ok(Statement::Import {
                import: ImportStatement::CrushModule {
                    module_path: name.name.to_string(),
                    alias: name.asname.as_ref().map(|s| s.to_string()),
                    selective: vec![],
                },
                meta,
            })
        }
        py_ast::Stmt::ImportFrom(py_ast::StmtImportFrom { module, names, .. }) => {
            let module_path = module.as_ref().map(|s| s.to_string()).unwrap_or_default();
            let selective: Vec<String> = names.iter().map(|n| n.name.to_string()).collect();
            Ok(Statement::Import {
                import: ImportStatement::CrushModule {
                    module_path,
                    alias: None,
                    selective,
                },
                meta,
            })
        }
        py_ast::Stmt::Pass { .. } => Ok(Statement::ExprStmt {
            expr: Expression::NullLiteral { meta: meta.clone() },
            meta,
        }),
        py_ast::Stmt::Global { .. } => anyhow::bail!("global keyword not yet supported"),
        py_ast::Stmt::Nonlocal { .. } => anyhow::bail!("nonlocal keyword not yet supported"),
        py_ast::Stmt::Delete { .. } => anyhow::bail!("del not yet supported"),
        py_ast::Stmt::Assert { .. } => anyhow::bail!("assert not yet supported"),
        py_ast::Stmt::ClassDef { .. } => anyhow::bail!("class definitions not yet supported"),
        py_ast::Stmt::With { .. } => anyhow::bail!("with statements not yet supported"),
        py_ast::Stmt::Match { .. } => anyhow::bail!("match statements not yet supported"),
        py_ast::Stmt::TypeAlias { .. } => anyhow::bail!("type aliases not yet supported"),
        py_ast::Stmt::AsyncFunctionDef { .. } => anyhow::bail!("async functions not yet supported"),
        py_ast::Stmt::AsyncFor { .. } => anyhow::bail!("async for not yet supported"),
        py_ast::Stmt::AsyncWith { .. } => anyhow::bail!("async with not yet supported"),
        py_ast::Stmt::AnnAssign { .. } => anyhow::bail!("annotated assignment not yet supported"),
        py_ast::Stmt::TryStar { .. } => anyhow::bail!("try* not yet supported"),
    }
}
