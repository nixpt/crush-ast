use std::collections::HashMap;

use crush_cast::{CastType, Expression, Function, ImportStatement, Program, Statement};
use swc_ecma_ast::*;
use crush_walker_core::LowerCtx;

fn meta(span: &swc_common::Span, ctx: &LowerCtx) -> HashMap<String, serde_json::Value> {
    let offset = span.lo.0 as usize;
    ctx.meta_at(if offset > 0 { offset } else { 0 })
}

fn wtf8_str<'a>(a: &'a swc_atoms::Wtf8Atom) -> &'a str {
    a.as_str().unwrap_or_default()
}

pub fn lower_module(module: &Module, ctx: &LowerCtx) -> anyhow::Result<Program> {
    let mut main_body: Vec<Statement> = Vec::new();
    let mut functions: HashMap<String, Function> = HashMap::new();

    for item in &module.body {
        match item {
            ModuleItem::Stmt(Stmt::Decl(Decl::Fn(fn_decl))) => {
                let name = fn_decl.ident.sym.to_string();
                let is_async = fn_decl.function.is_async;
                let lowered = lower_fn_decl(fn_decl, ctx)?;
                if let Statement::FunctionDef { params, body, .. } = lowered {
                    functions.insert(
                        name,
                        Function {
                            params,
                            body,
                            meta: HashMap::new(),
                            is_async,
                            ..Default::default()
                        },
                    );
                }
            }
            ModuleItem::Stmt(Stmt::Decl(Decl::Class(class_decl))) => {
                main_body.push(lower_class_decl(class_decl, ctx)?);
            }
            ModuleItem::Stmt(Stmt::Decl(Decl::Var(var_decl))) => {
                for decl in &var_decl.decls {
                    main_body.extend(lower_var_declarator(decl, ctx)?);
                }
            }
            ModuleItem::Stmt(stmt) => {
                if let Some(s) = lower_stmt(stmt, ctx)? {
                    main_body.push(s);
                }
            }
            ModuleItem::ModuleDecl(module_decl) => {
                if let Some(s) = lower_module_decl(module_decl, ctx)? {
                    main_body.push(s);
                }
            }
        }
    }

    if !main_body.is_empty() && !functions.contains_key("main") {
        functions.insert(
            "main".to_string(),
            Function {
                params: vec![],
                body: main_body,
                meta: HashMap::new(),
                ..Default::default()
            },
        );
    }

    Ok(Program {
        cast_version: "0.2".to_string(),
        entry: "main".to_string(),
        lang: Some("javascript".to_string()),
        functions,
        ai_meta: None,
        ..Default::default()
    })
}

fn lower_fn_decl(fn_decl: &FnDecl, ctx: &LowerCtx) -> anyhow::Result<Statement> {
    let params: Vec<(String, CastType)> = fn_decl
        .function
        .params
        .iter()
        .map(|p| (pat_to_name(&p.pat), CastType::Any))
        .collect();

    let mut body = Vec::new();
    if let Some(block) = &fn_decl.function.body {
        for s in &block.stmts {
            if let Some(stmt) = lower_stmt(s, ctx)? {
                body.push(stmt);
            }
        }
    }

    Ok(Statement::FunctionDef {
        name: fn_decl.ident.sym.to_string(),
        params,
        body,
        meta: meta(&fn_decl.function.span, ctx),
    })
}

fn lower_class_decl(class: &ClassDecl, ctx: &LowerCtx) -> anyhow::Result<Statement> {
    let name = class.ident.sym.to_string();

    let mut properties = Vec::new();
    for member in &class.class.body {
        match member {
            ClassMember::Method(method) => {
                let key = prop_name_to_string(&method.key);
                let params: Vec<(String, CastType)> = method
                    .function
                    .params
                    .iter()
                    .map(|p| (pat_to_name(&p.pat), CastType::Any))
                    .collect();
                let body = match &method.function.body {
                    Some(b) => stmts_to_vec(&b.stmts, ctx)?,
                    None => vec![],
                };
                properties.push((
                    key,
                    Expression::Lambda {
                        params,
                        body,
                        meta: meta(&method.function.span, ctx),
                    },
                ));
            }
            ClassMember::ClassProp(prop) => {
                let key = prop_name_to_string(&prop.key);
                let val = match &prop.value {
                    Some(v) => lower_expr(v, ctx)?,
                    None => Expression::NullLiteral { meta: meta(&prop.span, ctx) },
                };
                properties.push((key, val));
            }
            _ => {}
        }
    }

    Ok(Statement::VarDecl {
        name,
        value: Expression::ObjectLiteral {
            properties,
            meta: meta(&class.class.span, ctx),
        },
        type_hint: CastType::Any,
        meta: meta(&class.class.span, ctx),
    })
}

fn lower_module_decl(decl: &ModuleDecl, ctx: &LowerCtx) -> anyhow::Result<Option<Statement>> {
    match decl {
        ModuleDecl::Import(import) => {
            let mut selective = Vec::new();
            for specifier in &import.specifiers {
                match specifier {
                    ImportSpecifier::Named(named) => {
                        selective.push(named.local.sym.to_string());
                    }
                    ImportSpecifier::Default(default) => {
                        selective.push(default.local.sym.to_string());
                    }
                    ImportSpecifier::Namespace(ns) => {
                        selective.push(ns.local.sym.to_string());
                    }
                }
            }
            let src = wtf8_str(&import.src.value).to_string();
            Ok(Some(Statement::Import {
                import: ImportStatement::CrushModule {
                    module_path: src,
                    alias: None,
                    selective,
                },
                meta: meta(&import.span, ctx),
            }))
        }
        ModuleDecl::ExportDecl(export) => match &export.decl {
            Decl::Fn(fn_decl) => {
                let name = fn_decl.ident.sym.to_string();
                Ok(Some(Statement::Export {
                    name: name.clone(),
                    value: Expression::Var { name, meta: meta(&fn_decl.function.span, ctx) },
                    meta: meta(&export.span, ctx),
                }))
            }
            Decl::Var(var_decl) => {
                for d in &var_decl.decls {
                    let name = pat_to_name(&d.name);
                    return Ok(Some(Statement::Export {
                        name: name.clone(),
                        value: Expression::Var { name, meta: meta(&d.span, ctx) },
                        meta: meta(&export.span, ctx),
                    }));
                }
                Ok(None)
            }
            Decl::Class(class) => {
                let name = class.ident.sym.to_string();
                Ok(Some(Statement::Export {
                    name: name.clone(),
                    value: Expression::Var { name, meta: meta(&class.class.span, ctx) },
                    meta: meta(&export.span, ctx),
                }))
            }
            Decl::Using(using) => {
                for d in &using.decls {
                    let name = pat_to_name(&d.name);
                    return Ok(Some(Statement::Export {
                        name,
                        value: Expression::NullLiteral { meta: meta(&d.span, ctx) },
                        meta: meta(&export.span, ctx),
                    }));
                }
                Ok(None)
            }
            _ => Ok(Some(Statement::LangBlock {
                lang: "javascript".to_string(),
                code: "// export — not lowered".to_string(),
                variables: vec![],
                imports: vec![],
                meta: meta(&export.span, ctx),
            })),
        },
        ModuleDecl::ExportDefaultDecl(export_default) => match &export_default.decl {
            DefaultDecl::Fn(fn_expr) => {
                let params: Vec<(String, CastType)> = fn_expr
                    .function
                    .params
                    .iter()
                    .map(|p| (pat_to_name(&p.pat), CastType::Any))
                    .collect();
                let body = match &fn_expr.function.body {
                    Some(b) => stmts_to_vec(&b.stmts, ctx)?,
                    None => vec![],
                };
                Ok(Some(Statement::Export {
                    name: "default".to_string(),
                    value: Expression::Lambda {
                        params,
                        body,
                        meta: meta(&fn_expr.function.span, ctx),
                    },
                    meta: meta(&export_default.span, ctx),
                }))
            }
            DefaultDecl::Class(class) => {
                let name = class
                    .ident
                    .as_ref()
                    .map(|id| id.sym.to_string())
                    .unwrap_or_default();
                Ok(Some(Statement::Export {
                    name: "default".to_string(),
                    value: Expression::Var { name, meta: meta(&class.class.span, ctx) },
                    meta: meta(&export_default.span, ctx),
                }))
            }
            _ => Ok(Some(Statement::LangBlock {
                lang: "javascript".to_string(),
                code: "// export default — not lowered".to_string(),
                variables: vec![],
                imports: vec![],
                meta: meta(&export_default.span, ctx),
            })),
        },
        ModuleDecl::ExportDefaultExpr(expr) => {
            let val = lower_expr(&expr.expr, ctx)?;
            Ok(Some(Statement::Export {
                name: "default".to_string(),
                value: val,
                meta: meta(&expr.span, ctx),
            }))
        }
        ModuleDecl::ExportNamed(named) => {
            let src = named.src.as_ref().map(|s| wtf8_str(&s.value).to_string());
            if let Some(ref module) = src {
                // re-export from another module: emit as import
                let mut selective = Vec::new();
                for spec in &named.specifiers {
                    if let ExportSpecifier::Named(ns) = spec {
                        selective.push(ns.orig.atom().to_string());
                    }
                }
                Ok(Some(Statement::Import {
                    import: ImportStatement::CrushModule {
                        module_path: module.clone(),
                        alias: None,
                        selective,
                    },
                    meta: meta(&named.span, ctx),
                }))
            } else {
                // direct named exports
                for spec in &named.specifiers {
                    if let ExportSpecifier::Named(ns) = spec {
                        let name = ns
                            .exported
                            .as_ref()
                            .map(|m| m.atom().to_string())
                            .unwrap_or_else(|| ns.orig.atom().to_string());
                        let orig = ns.orig.atom().to_string();
                        return Ok(Some(Statement::Export {
                            name,
                            value: Expression::Var {
                                name: orig,
                                meta: meta(&named.span, ctx),
                            },
                            meta: meta(&named.span, ctx),
                        }));
                    }
                }
                Ok(None)
            }
        }
        ModuleDecl::ExportAll(all) => {
            let src = wtf8_str(&all.src.value).to_string();
            Ok(Some(Statement::Import {
                import: ImportStatement::CrushModule {
                    module_path: src,
                    alias: None,
                    selective: vec![],
                },
                meta: meta(&all.span, ctx),
            }))
        }
        ModuleDecl::TsImportEquals(ts) => {
            let mod_name = match &ts.module_ref {
                TsModuleRef::TsEntityName(e) => match e {
                    TsEntityName::Ident(id) => id.sym.to_string(),
                    TsEntityName::TsQualifiedName(q) => match &q.left {
                        TsEntityName::Ident(id) => id.sym.to_string(),
                        _ => "ts-qualified".to_string(),
                    },
                },
                TsModuleRef::TsExternalModuleRef(ext) => wtf8_str(&ext.expr.value).to_string(),
            };
            Ok(Some(Statement::LangBlock {
                lang: "javascript".to_string(),
                code: format!("// ts import equals: {}", mod_name),
                variables: vec![],
                imports: vec![],
                meta: meta(&ts.span, ctx),
            }))
        }
        ModuleDecl::TsExportAssignment(ts) => {
            let val = lower_expr(&ts.expr, ctx)?;
            Ok(Some(Statement::Export {
                name: "default".to_string(),
                value: val,
                meta: meta(&ts.span, ctx),
            }))
        }
        ModuleDecl::TsNamespaceExport(ts) => Ok(Some(Statement::LangBlock {
            lang: "javascript".to_string(),
            code: format!("// ts namespace export: {}", ts.id.sym),
            variables: vec![],
            imports: vec![],
            meta: meta(&ts.span, ctx),
        })),
    }
}

fn lower_stmt(stmt: &Stmt, ctx: &LowerCtx) -> anyhow::Result<Option<Statement>> {
    Ok(Some(match stmt {
        Stmt::Expr(ExprStmt { expr, span, .. }) => Statement::ExprStmt {
            expr: lower_expr(expr, ctx)?,
            meta: meta(span, ctx),
        },
        Stmt::Decl(decl) => match decl {
            Decl::Fn(fn_decl) => lower_fn_decl(fn_decl, ctx)?,
            Decl::Var(var_decl) => {
                let mut stmts = Vec::new();
                for d in &var_decl.decls {
                    stmts.extend(lower_var_declarator(d, ctx)?);
                }
                if stmts.is_empty() {
                    return Ok(None);
                }
                stmts.into_iter().next().unwrap()
            }
            Decl::Class(class_decl) => lower_class_decl(class_decl, ctx)?,
            Decl::Using(using) => {
                let mut stmts = Vec::new();
                for d in &using.decls {
                    let name = pat_to_name(&d.name);
                    let value = match &d.init {
                        Some(init) => lower_expr(init, ctx)?,
                        None => Expression::NullLiteral { meta: meta(&d.span, ctx) },
                    };
                    stmts.push(Statement::VarDecl {
                        name,
                        value,
                        type_hint: CastType::Any,
                        meta: meta(&d.span, ctx),
                    });
                }
                if stmts.is_empty() {
                    return Ok(None);
                }
                stmts.into_iter().next().unwrap()
            }
            Decl::TsInterface(_) | Decl::TsTypeAlias(_) | Decl::TsEnum(_) | Decl::TsModule(_) => {
                return Ok(None);
            }
        },
        Stmt::Return(ReturnStmt { arg, span, .. }) => {
            let value = match arg {
                Some(expr) => Some(lower_expr(expr, ctx)?),
                None => None,
            };
            Statement::Return {
                value,
                meta: meta(span, ctx),
            }
        }
        Stmt::Throw(ThrowStmt { arg, span, .. }) => Statement::Throw {
            value: lower_expr(arg, ctx)?,
            meta: meta(span, ctx),
        },
        Stmt::If(IfStmt {
            test, cons, alt, span, ..
        }) => {
            let condition = lower_expr(test, ctx)?;
            let then_body = block_or_stmt_to_vec(cons, ctx)?;
            let else_body = match alt {
                Some(alt) => Some(block_or_stmt_to_vec(alt, ctx)?),
                None => None,
            };
            Statement::If {
                condition,
                then_body,
                else_body,
                meta: meta(span, ctx),
            }
        }
        Stmt::While(WhileStmt { test, body, span, .. }) => {
            let condition = lower_expr(test, ctx)?;
            let body = block_or_stmt_to_vec(body, ctx)?;
            Statement::While {
                condition: Box::new(condition),
                body,
                meta: meta(span, ctx),
            }
        }
        Stmt::DoWhile(DoWhileStmt { test, body, span, .. }) => {
            let condition = lower_expr(test, ctx)?;
            let body = block_or_stmt_to_vec(body, ctx)?;
            Statement::While {
                condition: Box::new(condition),
                body,
                meta: meta(span, ctx),
            }
        }
        Stmt::For(ForStmt {
            init,
            test,
            update,
            body,
            span,
            ..
        }) => {
            let variable = match init {
                Some(_) => "i".to_string(),
                None => "i".to_string(),
            };
            let test_expr = match test {
                Some(t) => lower_expr(t, ctx)?,
                None => Expression::BoolLiteral {
                    value: true,
                    meta: meta(span, ctx),
                },
            };
            let mut body_stmts = block_or_stmt_to_vec(body, ctx)?;
            if let Some(upd) = update {
                let update_expr = lower_expr(upd, ctx)?;
                body_stmts.push(Statement::ExprStmt {
                    expr: update_expr,
                    meta: meta(&swc_common::DUMMY_SP, ctx),
                });
            }
            body_stmts.insert(
                0,
                Statement::If {
                    condition: Expression::UnaryOp {
                        operator: "!".to_string(),
                        operand: Box::new(test_expr),
                        meta: meta(span, ctx),
                    },
                    then_body: vec![Statement::Break { meta: meta(span, ctx) }],
                    else_body: None,
                    meta: meta(span, ctx),
                },
            );
            Statement::For {
                variable,
                iterable: Box::new(Expression::Call {
                    function: "make_range".to_string(),
                    args: vec![],
                    meta: meta(span, ctx),
                }),
                body: body_stmts,
                meta: meta(span, ctx),
            }
        }
        Stmt::ForIn(ForInStmt {
            left, right, body, span, ..
        }) => {
            let variable = for_head_to_var(left);
            let iterable = lower_expr(right, ctx)?;
            Statement::For {
                variable,
                iterable: Box::new(iterable),
                body: block_or_stmt_to_vec(body, ctx)?,
                meta: meta(span, ctx),
            }
        }
        Stmt::ForOf(ForOfStmt {
            left, right, body, span, ..
        }) => {
            let variable = for_head_to_var(left);
            let iterable = lower_expr(right, ctx)?;
            Statement::For {
                variable,
                iterable: Box::new(iterable),
                body: block_or_stmt_to_vec(body, ctx)?,
                meta: meta(span, ctx),
            }
        }
        Stmt::Try(box_try) => {
            let TryStmt {
                block,
                handler,
                finalizer: _,
                span,
                ..
            } = box_try.as_ref();
            let body = stmts_to_vec(&block.stmts, ctx)?;
            let (error_var, handler_body) = match handler {
                Some(catch) => {
                    let var = catch
                        .param
                        .as_ref()
                        .map(|p| pat_to_name(p))
                        .unwrap_or_else(|| "err".to_string());
                    (var, stmts_to_vec(&catch.body.stmts, ctx)?)
                }
                None => ("err".to_string(), vec![]),
            };
            Statement::TryCatch {
                body,
                error_var,
                handler: handler_body,
                meta: meta(span, ctx),
            }
        }
        Stmt::Switch(SwitchStmt {
            discriminant,
            cases,
            span,
            ..
        }) => {
            let cond = lower_expr(discriminant, ctx)?;
            let mut body = Vec::new();
            body.push(Statement::VarDecl {
                name: "__switch_val".to_string(),
                value: cond,
                type_hint: CastType::Any,
                meta: meta(span, ctx),
            });
            for case in cases {
                let test = match &case.test {
                    Some(t) => lower_expr(t, ctx)?,
                    None => Expression::BoolLiteral {
                        value: true,
                        meta: meta(&case.span, ctx),
                    },
                };
                body.push(Statement::If {
                    condition: Expression::BinaryOp {
                        operator: "===".to_string(),
                        left: Box::new(Expression::Var {
                            name: "__switch_val".to_string(),
                            meta: meta(&case.span, ctx),
                        }),
                        right: Box::new(test),
                        meta: meta(&case.span, ctx),
                    },
                    then_body: stmts_to_vec(&case.cons, ctx)?,
                    else_body: None,
                    meta: meta(&case.span, ctx),
                });
            }
            Statement::ExprStmt {
                expr: Expression::NullLiteral { meta: meta(&swc_common::DUMMY_SP, ctx) },
                meta: meta(span, ctx),
            }
        }
        Stmt::Break(bs) => Statement::Break { meta: meta(&bs.span, ctx) },
        Stmt::Continue(cs) => Statement::Continue { meta: meta(&cs.span, ctx) },
        Stmt::With(_ws) => return Ok(None),
        Stmt::Labeled(LabeledStmt { body, .. }) => {
            return lower_stmt(body, ctx);
        }
        Stmt::Block(_) | Stmt::Debugger(_) | Stmt::Empty(_) => return Ok(None),
    }))
}

fn lower_var_declarator(decl: &VarDeclarator, ctx: &LowerCtx) -> anyhow::Result<Vec<Statement>> {
    let name = pat_to_name(&decl.name);
    let value = match &decl.init {
        Some(init) => lower_expr(init, ctx)?,
        None => Expression::NullLiteral { meta: meta(&decl.span, ctx) },
    };
    Ok(vec![Statement::VarDecl {
        name,
        value,
        type_hint: CastType::Any,
        meta: meta(&decl.span, ctx),
    }])
}

fn for_head_to_var(head: &ForHead) -> String {
    match head {
        ForHead::VarDecl(var_decl) => var_decl
            .decls
            .first()
            .map(|d| pat_to_name(&d.name))
            .unwrap_or_else(|| "i".to_string()),
        ForHead::Pat(pat) => pat_to_name(pat),
        ForHead::UsingDecl(using) => using
            .decls
            .first()
            .map(|d| pat_to_name(&d.name))
            .unwrap_or_else(|| "i".to_string()),
    }
}

fn pat_to_name(pat: &Pat) -> String {
    match pat {
        Pat::Ident(binding) => binding.id.sym.to_string(),
        Pat::Array(_) => "_arr".to_string(),
        Pat::Object(_) => "_obj".to_string(),
        Pat::Assign(assign) => pat_to_name(&assign.left),
        Pat::Rest(rest) => pat_to_name(&rest.arg),
        Pat::Expr(expr) => {
            if let Expr::Ident(i) = expr.as_ref() {
                i.sym.to_string()
            } else {
                "_expr".to_string()
            }
        }
        Pat::Invalid(_) => "_invalid".to_string(),
    }
}

fn block_or_stmt_to_vec(stmt: &Stmt, ctx: &LowerCtx) -> anyhow::Result<Vec<Statement>> {
    match stmt {
        Stmt::Block(block) => stmts_to_vec(&block.stmts, ctx),
        other => {
            if let Some(s) = lower_stmt(other, ctx)? {
                Ok(vec![s])
            } else {
                Ok(vec![])
            }
        }
    }
}

fn stmts_to_vec(stmts: &[Stmt], ctx: &LowerCtx) -> anyhow::Result<Vec<Statement>> {
    let mut result = Vec::new();
    for s in stmts {
        match s {
            Stmt::Block(b) => {
                result.extend(stmts_to_vec(&b.stmts, ctx)?);
            }
            _ => {
                if let Some(stmt) = lower_stmt(s, ctx)? {
                    result.push(stmt);
                }
            }
        }
    }
    Ok(result)
}

pub fn lower_expr(expr: &Expr, ctx: &LowerCtx) -> anyhow::Result<Expression> {
    let m = ctx.meta_at(0);
    match expr {
        Expr::Ident(ident) => Ok(Expression::Var {
            name: ident.sym.to_string(),
            meta: m,
        }),
        Expr::Lit(lit) => lower_lit(lit, ctx),
        Expr::Unary(UnaryExpr { op, arg, .. }) => {
            let operand = lower_expr(arg, ctx)?;
            let operator = match op {
                UnaryOp::Minus => "-",
                UnaryOp::Plus => "+",
                UnaryOp::Bang => "!",
                UnaryOp::Tilde => "~",
                UnaryOp::TypeOf => "typeof",
                UnaryOp::Void => "void",
                UnaryOp::Delete => "delete",
            };
            Ok(Expression::UnaryOp {
                operator: operator.to_string(),
                operand: Box::new(operand),
                meta: m,
            })
        }
        Expr::Bin(BinExpr {
            op, left, right, ..
        }) => {
            let left = lower_expr(left, ctx)?;
            let right = lower_expr(right, ctx)?;
            let operator = match op {
                BinaryOp::EqEq => "==",
                BinaryOp::NotEq => "!=",
                BinaryOp::EqEqEq => "===",
                BinaryOp::NotEqEq => "!==",
                BinaryOp::Lt => "<",
                BinaryOp::LtEq => "<=",
                BinaryOp::Gt => ">",
                BinaryOp::GtEq => ">=",
                BinaryOp::LShift => "<<",
                BinaryOp::RShift => ">>",
                BinaryOp::ZeroFillRShift => ">>>",
                BinaryOp::Add => "+",
                BinaryOp::Sub => "-",
                BinaryOp::Mul => "*",
                BinaryOp::Div => "/",
                BinaryOp::Mod => "%",
                BinaryOp::BitOr => "|",
                BinaryOp::BitXor => "^",
                BinaryOp::BitAnd => "&",
                BinaryOp::LogicalOr => "||",
                BinaryOp::LogicalAnd => "&&",
                BinaryOp::In => "in",
                BinaryOp::InstanceOf => "instanceof",
                BinaryOp::Exp => "**",
                BinaryOp::NullishCoalescing => "??",
            };
            Ok(Expression::BinaryOp {
                operator: operator.to_string(),
                left: Box::new(left),
                right: Box::new(right),
                meta: m,
            })
        }
        Expr::Cond(CondExpr {
            test, cons, alt, ..
        }) => {
            let test = lower_expr(test, ctx)?;
            let cons = lower_expr(cons, ctx)?;
            let alt = lower_expr(alt, ctx)?;
            Ok(Expression::Call {
                function: "__crush_ifexpr__".to_string(),
                args: vec![test, cons, alt],
                meta: m,
            })
        }
        Expr::Call(CallExpr { callee, args, .. }) => lower_call_expr(callee, args, m, ctx),
        Expr::New(NewExpr { callee, args, span: _, .. }) => {
            let callee_str = match callee.as_ref() {
                Expr::Ident(i) => i.sym.to_string(),
                _ => "Object".to_string(),
            };
            let mut lowered_args = Vec::new();
            if let Some(args_list) = args {
                for a in args_list {
                    lowered_args.push(lower_expr(&a.expr, ctx)?);
                }
            }
            Ok(Expression::Call {
                function: format!("new {}", callee_str),
                args: lowered_args,
                meta: m,
            })
        }
        Expr::Member(MemberExpr { obj, prop, span: _, .. }) => {
            let target = lower_expr(obj, ctx)?;
            match prop {
                MemberProp::Ident(ident_name) => Ok(Expression::GetField {
                    target: Box::new(target),
                    field: ident_name.sym.to_string(),
                    meta: m,
                }),
                MemberProp::Computed(computed) => {
                    let index = lower_expr(&computed.expr, ctx)?;
                    Ok(Expression::Index {
                        target: Box::new(target),
                        index: Box::new(index),
                        meta: m,
                    })
                }
                MemberProp::PrivateName(pn) => Ok(Expression::GetField {
                    target: Box::new(target),
                    field: format!("#{}", pn.name),
                    meta: m,
                }),
            }
        }
        Expr::Array(ArrayLit { elems, .. }) => {
            let mut elements = Vec::new();
            for elem in elems {
                match elem {
                    Some(expr_or_spread) => {
                        elements.push(lower_expr(&expr_or_spread.expr, ctx)?);
                    }
                    None => {
                        elements.push(Expression::NullLiteral { meta: m.clone() });
                    }
                }
            }
            Ok(Expression::ArrayLiteral { elements, meta: m })
        }
        Expr::Object(ObjectLit { props, .. }) => {
            let mut properties = Vec::new();
            for prop in props {
                match prop {
                    PropOrSpread::Prop(box_prop) => match box_prop.as_ref() {
                        Prop::KeyValue(kv) => {
                            let key = prop_name_to_string(&kv.key);
                            let val = lower_expr(&kv.value, ctx)?;
                            properties.push((key, val));
                        }
                        Prop::Shorthand(ident) => {
                            let key = ident.sym.to_string();
                            properties.push((
                                key,
                                Expression::Var {
                                    name: ident.sym.to_string(),
                                    meta: m.clone(),
                                },
                            ));
                        }
                        Prop::Method(method) => {
                            let key = prop_name_to_string(&method.key);
                            let params: Vec<(String, CastType)> = method
                                .function
                                .params
                                .iter()
                                .map(|p| (pat_to_name(&p.pat), CastType::Any))
                                .collect();
                            let body = match &method.function.body {
                                Some(b) => stmts_to_vec(&b.stmts, ctx)?,
                                None => vec![],
                            };
                            properties.push((
                                key,
                                Expression::Lambda {
                                    params,
                                    body,
                                    meta: m.clone(),
                                },
                            ));
                        }
                        Prop::Getter(getter) => {
                            let key = prop_name_to_string(&getter.key);
                            let body = match &getter.body {
                                Some(b) => stmts_to_vec(&b.stmts, ctx)?,
                                None => vec![],
                            };
                            properties.push((
                                key,
                                Expression::Lambda {
                                    params: vec![],
                                    body,
                                    meta: m.clone(),
                                },
                            ));
                        }
                        Prop::Setter(setter) => {
                            let key = prop_name_to_string(&setter.key);
                            let param_name = pat_to_name(&setter.param);
                            properties.push((
                                key,
                                Expression::Lambda {
                                    params: vec![(param_name, CastType::Any)],
                                    body: vec![],
                                    meta: m.clone(),
                                },
                            ));
                        }
                        Prop::Assign(_) => {}
                    },
                    PropOrSpread::Spread(_) => {}
                }
            }
            Ok(Expression::ObjectLiteral {
                properties,
                meta: m,
            })
        }
        Expr::Arrow(ArrowExpr { params, body, .. }) => {
            let params: Vec<(String, CastType)> = params
                .iter()
                .map(|p| (pat_to_name(p), CastType::Any))
                .collect();
            let body = match body.as_ref() {
                BlockStmtOrExpr::BlockStmt(block) => stmts_to_vec(&block.stmts, ctx)?,
                BlockStmtOrExpr::Expr(expr) => {
                    vec![Statement::Return {
                        value: Some(lower_expr(expr, ctx)?),
                        meta: m.clone(),
                    }]
                }
            };
            Ok(Expression::Lambda {
                params,
                body,
                meta: m,
            })
        }
        Expr::Fn(FnExpr {
            ident: _, function, ..
        }) => {
            let params: Vec<(String, CastType)> = function
                .params
                .iter()
                .map(|p| (pat_to_name(&p.pat), CastType::Any))
                .collect();
            let body = match &function.body {
                Some(b) => stmts_to_vec(&b.stmts, ctx)?,
                None => vec![],
            };
            Ok(Expression::Lambda {
                params,
                body,
                meta: m,
            })
        }
        Expr::Assign(AssignExpr {
            op, left, right, ..
        }) => {
            let right = lower_expr(right, ctx)?;
            let value = match op {
                AssignOp::Assign => right,
                _ => {
                    let name = assign_target_to_name(left);
                    Expression::BinaryOp {
                        operator: match op {
                            AssignOp::AddAssign => "+", AssignOp::SubAssign => "-",
                            AssignOp::MulAssign => "*", AssignOp::DivAssign => "/",
                            AssignOp::ModAssign => "%", AssignOp::LShiftAssign => "<<",
                            AssignOp::RShiftAssign => ">>", AssignOp::BitOrAssign => "|",
                            AssignOp::BitXorAssign => "^", AssignOp::BitAndAssign => "&",
                            AssignOp::ExpAssign => "**", AssignOp::AndAssign => "&&",
                            AssignOp::OrAssign => "||", AssignOp::NullishAssign => "??",
                            _ => "=",
                        }.to_string(),
                        left: Box::new(Expression::Var { name: name.clone(), meta: m.clone() }),
                        right: Box::new(right),
                        meta: m.clone(),
                    }
                }
            };
            // Subscript assignment: arr[i] = val -> __crush_setindex__(arr, i, val)
            if let Some((obj_expr, idx_expr)) = assign_target_subscript_parts(left) {
                let obj = lower_expr(obj_expr, ctx)?;
                let idx = lower_expr(idx_expr, ctx)?;
                return Ok(Expression::Call {
                    function: "__crush_setindex__".to_string(),
                    args: vec![obj, idx, value],
                    meta: m,
                });
            }
            let name = assign_target_to_name(left);
            Ok(Expression::Call {
                function: "__crush_assign__".to_string(),
                args: vec![
                    Expression::Var { name, meta: m.clone() },
                    value,
                ],
                meta: m,
            })
        }
        Expr::Seq(SeqExpr { exprs, .. }) => {
            let mut last = Expression::NullLiteral { meta: m.clone() };
            for e in exprs {
                last = lower_expr(e, ctx)?;
            }
            Ok(last)
        }
        Expr::Tpl(Tpl { exprs, quasis, .. }) => {
            if exprs.is_empty() {
                let val = quasis
                    .iter()
                    .map(|q| q.raw.to_string())
                    .collect::<Vec<_>>()
                    .join("");
                return Ok(Expression::StringLiteral {
                    value: val,
                    meta: m,
                });
            }
            let mut result = Expression::StringLiteral {
                value: quasis
                    .first()
                    .map(|q| q.raw.to_string())
                    .unwrap_or_default(),
                meta: m.clone(),
            };
            for (i, expr) in exprs.iter().enumerate() {
                let expr = lower_expr(expr, ctx)?;
                result = Expression::BinaryOp {
                    operator: "+".to_string(),
                    left: Box::new(result),
                    right: Box::new(expr),
                    meta: m.clone(),
                };
                if let Some(quasi) = quasis.get(i + 1) {
                    let text = Expression::StringLiteral {
                        value: quasi.raw.to_string(),
                        meta: m.clone(),
                    };
                    result = Expression::BinaryOp {
                        operator: "+".to_string(),
                        left: Box::new(result),
                        right: Box::new(text),
                        meta: m.clone(),
                    };
                }
            }
            Ok(result)
        }
        Expr::TaggedTpl(TaggedTpl { tag, tpl, .. }) => {
            let tag_str = match tag.as_ref() {
                Expr::Ident(i) => i.sym.to_string(),
                Expr::Member(m) => {
                    let obj = match m.obj.as_ref() {
                        Expr::Ident(i) => i.sym.to_string(),
                        _ => "_".to_string(),
                    };
                    match &m.prop {
                        MemberProp::Ident(i) => format!("{}.{}", obj, i.sym),
                        _ => "tagged_template".to_string(),
                    }
                }
                _ => "tagged_template".to_string(),
            };
            let mut args: Vec<Expression> = tpl
                .quasis
                .iter()
                .map(|q| Expression::StringLiteral {
                    value: q.raw.to_string(),
                    meta: m.clone(),
                })
                .collect();
            for expr in &tpl.exprs {
                args.push(lower_expr(expr, ctx)?);
            }
            Ok(Expression::Call {
                function: tag_str,
                args,
                meta: m,
            })
        }
        Expr::Await(AwaitExpr { arg, .. }) => {
            let arg = lower_expr(arg, ctx)?;
            Ok(Expression::Await {
                expression: Box::new(arg),
                meta: m,
            })
        }
        Expr::Yield(YieldExpr { arg, .. }) => {
            if let Some(arg) = arg {
                lower_expr(arg, ctx)
            } else {
                Ok(Expression::NullLiteral { meta: m })
            }
        }
        Expr::Update(UpdateExpr {
            op, arg, prefix, span: _, ..
        }) => {
            let name = match arg.as_ref() {
                Expr::Ident(i) => i.sym.to_string(),
                _ => "_expr".to_string(),
            };
            let fname = match (op, prefix) {
                (UpdateOp::PlusPlus, true) => "__crush_pre_inc__",
                (UpdateOp::MinusMinus, true) => "__crush_pre_dec__",
                (UpdateOp::PlusPlus, false) => "__crush_post_inc__",
                (UpdateOp::MinusMinus, false) => "__crush_post_dec__",
            };
            Ok(Expression::Call {
                function: fname.to_string(),
                args: vec![Expression::Var {
                    name,
                    meta: m.clone(),
                }],
                meta: m,
            })
        }
        Expr::SuperProp(SuperPropExpr { obj: _, prop, span: _, .. }) => {
            let field = match prop {
                SuperProp::Ident(i) => i.sym.to_string(),
                SuperProp::Computed(_) => "[]".to_string(),
            };
            Ok(Expression::GetField {
                target: Box::new(Expression::Var {
                    name: "super".to_string(),
                    meta: m.clone(),
                }),
                field,
                meta: m,
            })
        }
        Expr::This(ThisExpr { span: _, .. }) => Ok(Expression::Var {
            name: "this".to_string(),
            meta: m,
        }),
        Expr::MetaProp(_) => Ok(Expression::Var {
            name: "import.meta".to_string(),
            meta: m,
        }),
        Expr::PrivateName(priv_name) => Ok(Expression::GetField {
            target: Box::new(Expression::Var {
                name: "this".to_string(),
                meta: m.clone(),
            }),
            field: format!("#{}", priv_name.name),
            meta: m,
        }),
        Expr::Paren(ParenExpr { expr, .. }) => lower_expr(expr, ctx),
        Expr::TsTypeAssertion(TsTypeAssertion { expr, .. }) => lower_expr(expr, ctx),
        Expr::TsAs(TsAsExpr { expr, .. }) => lower_expr(expr, ctx),
        Expr::TsNonNull(TsNonNullExpr { expr, .. }) => lower_expr(expr, ctx),
        Expr::TsSatisfies(TsSatisfiesExpr { expr, .. }) => lower_expr(expr, ctx),
        Expr::TsConstAssertion(TsConstAssertion { expr, .. }) => lower_expr(expr, ctx),
        Expr::TsInstantiation(TsInstantiation { expr, .. }) => lower_expr(expr, ctx),
        Expr::OptChain(OptChainExpr { base, .. }) => match base.as_ref() {
            OptChainBase::Member(member) => {
                let target = lower_expr(&member.obj, ctx)?;
                match &member.prop {
                    MemberProp::Ident(i) => Ok(Expression::GetField {
                        target: Box::new(target),
                        field: i.sym.to_string(),
                        meta: m,
                    }),
                    MemberProp::Computed(c) => {
                        let index = lower_expr(&c.expr, ctx)?;
                        Ok(Expression::Index {
                            target: Box::new(target),
                            index: Box::new(index),
                            meta: m,
                        })
                    }
                    MemberProp::PrivateName(pn) => Ok(Expression::GetField {
                        target: Box::new(target),
                        field: format!("#{}", pn.name),
                        meta: m,
                    }),
                }
            }
            OptChainBase::Call(call) => {
                let callee = Callee::Expr(call.callee.clone());
                lower_call_expr(&callee, &call.args, m, ctx)
            }
        },
        Expr::Class(ClassExpr { class, .. }) => {
            let mut properties = Vec::new();
            for member in &class.body {
                if let ClassMember::Method(method) = member {
                    let key = prop_name_to_string(&method.key);
                    let params: Vec<(String, CastType)> = method
                        .function
                        .params
                        .iter()
                        .map(|p| (pat_to_name(&p.pat), CastType::Any))
                        .collect();
                    let body = match &method.function.body {
                        Some(b) => stmts_to_vec(&b.stmts, ctx)?,
                        None => vec![],
                    };
                    properties.push((
                        key,
                        Expression::Lambda {
                            params,
                            body,
                            meta: m.clone(),
                        },
                    ));
                }
            }
            Ok(Expression::ObjectLiteral {
                properties,
                meta: m,
            })
        }
        Expr::JSXElement(_)
        | Expr::JSXFragment(_)
        | Expr::JSXMember(_)
        | Expr::JSXNamespacedName(_)
        | Expr::JSXEmpty(_) => Ok(Expression::Call {
            function: "__crush_jsx__".to_string(),
            args: vec![],
            meta: m,
        }),
        Expr::Invalid(_) => {
            anyhow::bail!("invalid expression in source code")
        }
    }
}

fn lower_call_expr(
    callee: &Callee,
    args: &[ExprOrSpread],
    m: HashMap<String, serde_json::Value>,
    ctx: &LowerCtx,
) -> anyhow::Result<Expression> {
    let lowered_args: Vec<Expression> = args
        .iter()
        .map(|a| lower_expr(&a.expr, ctx))
        .collect::<anyhow::Result<Vec<_>>>()?;

    match callee {
        Callee::Expr(callee_expr) => {
            let expr = callee_expr.as_ref();
            let func_name = match expr {
                Expr::Ident(ident) => ident.sym.to_string(),
                Expr::Member(MemberExpr { obj, prop, .. }) => {
                    let obj_str = match obj.as_ref() {
                        Expr::Ident(i) => i.sym.to_string(),
                        _ => "_expr".to_string(),
                    };
                    let prop_str = match prop {
                        MemberProp::Ident(i) => i.sym.to_string(),
                        MemberProp::Computed(_) => "[]".to_string(),
                        MemberProp::PrivateName(pn) => format!("#{}", pn.name),
                    };
                    format!("{}.{}", obj_str, prop_str)
                }
                _ => "__crush_call__".to_string(),
            };

            match func_name.as_str() {
                "console.log" | "console.info" | "console.warn" | "console.error" => {
                    Ok(Expression::CapabilityCall {
                        name: "io.print".to_string(),
                        args: lowered_args,
                        meta: m,
                    })
                }
                "fetch" => Ok(Expression::CapabilityCall {
                    name: "net.http_get".to_string(),
                    args: lowered_args,
                    meta: m,
                }),
                "parseInt" | "parseFloat" | "Number" | "String" | "Boolean" | "Array"
                | "Object" => Ok(Expression::Call {
                    function: func_name,
                    args: lowered_args,
                    meta: m,
                }),
                "Array.isArray" => Ok(Expression::Call {
                    function: "is_array".to_string(),
                    args: lowered_args,
                    meta: m,
                }),
                "Math.max" | "Math.min" | "Math.abs" | "Math.floor" | "Math.ceil"
                | "Math.round" | "Math.sqrt" | "Math.pow" | "Math.random" => Ok(Expression::Call {
                    function: func_name,
                    args: lowered_args,
                    meta: m,
                }),
                "JSON.parse" => Ok(Expression::Call {
                    function: "json_parse".to_string(),
                    args: lowered_args,
                    meta: m,
                }),
                "JSON.stringify" => Ok(Expression::Call {
                    function: "json_stringify".to_string(),
                    args: lowered_args,
                    meta: m,
                }),
                _ => Ok(Expression::Call {
                    function: func_name,
                    args: lowered_args,
                    meta: m,
                }),
            }
        }
        Callee::Super(_) => Ok(Expression::Call {
            function: "super".to_string(),
            args: lowered_args,
            meta: m,
        }),
        Callee::Import(_) => Ok(Expression::Call {
            function: "import".to_string(),
            args: lowered_args,
            meta: m,
        }),
    }
}

fn lower_lit(lit: &Lit, ctx: &LowerCtx) -> anyhow::Result<Expression> {
    let m = ctx.meta_at(0);
    match lit {
        Lit::Str(s) => Ok(Expression::StringLiteral {
            value: wtf8_str(&s.value).to_string(),
            meta: m,
        }),
        Lit::Num(n) => {
            if n.value.fract() == 0.0 && n.value.abs() <= (i64::MAX as f64) {
                Ok(Expression::IntLiteral {
                    value: n.value as i64,
                    meta: m,
                })
            } else {
                Ok(Expression::FloatLiteral {
                    value: n.value,
                    meta: m,
                })
            }
        }
        Lit::Bool(b) => Ok(Expression::BoolLiteral {
            value: b.value,
            meta: m,
        }),
        Lit::Null(_) => Ok(Expression::NullLiteral { meta: m }),
        Lit::Regex(regex) => Ok(Expression::Call {
            function: "RegExp".to_string(),
            args: vec![
                Expression::StringLiteral {
                    value: regex.exp.to_string(),
                    meta: m.clone(),
                },
                Expression::StringLiteral {
                    value: regex.flags.to_string(),
                    meta: m.clone(),
                },
            ],
            meta: m,
        }),
        Lit::BigInt(bi) => {
            let val = bi.value.to_string();
            Ok(Expression::Call {
                function: "BigInt".to_string(),
                args: vec![Expression::StringLiteral {
                    value: val,
                    meta: m.clone(),
                }],
                meta: m,
            })
        }
        Lit::JSXText(jsx_text) => Ok(Expression::StringLiteral {
            value: jsx_text.value.to_string(),
            meta: m,
        }),
    }
}

fn assign_target_to_name(target: &AssignTarget) -> String {
    match target {
        AssignTarget::Simple(simple) => simple_assign_target_name(simple),
        AssignTarget::Pat(pat) => match pat {
            AssignTargetPat::Array(_) => "_arr".to_string(),
            AssignTargetPat::Object(_) => "_obj".to_string(),
            AssignTargetPat::Invalid(_) => "_invalid".to_string(),
        },
    }
}

fn simple_assign_target_name(target: &SimpleAssignTarget) -> String {
    match target {
        SimpleAssignTarget::Ident(binding) => binding.id.sym.to_string(),
        SimpleAssignTarget::Member(member) => {
            let obj = match member.obj.as_ref() {
                Expr::Ident(i) => i.sym.to_string(),
                _ => "_expr".to_string(),
            };
            let prop = match &member.prop {
                MemberProp::Ident(i) => i.sym.to_string(),
                MemberProp::Computed(_) => "[]".to_string(),
                MemberProp::PrivateName(pn) => format!("#{}", pn.name),
            };
            format!("{}.{}", obj, prop)
        }
        SimpleAssignTarget::SuperProp(sp) => {
            let prop_str = match &sp.prop {
                SuperProp::Ident(i) => i.sym.to_string(),
                SuperProp::Computed(_) => "[]".to_string(),
            };
            format!("super.{}", prop_str)
        }
        SimpleAssignTarget::Paren(p) => {
            if let Expr::Ident(i) = p.expr.as_ref() {
                i.sym.to_string()
            } else {
                "_expr".to_string()
            }
        }
        SimpleAssignTarget::TsAs(ts_as) => {
            if let Expr::Ident(i) = ts_as.expr.as_ref() {
                i.sym.to_string()
            } else {
                "_expr".to_string()
            }
        }
        SimpleAssignTarget::TsNonNull(non_null) => {
            if let Expr::Ident(i) = non_null.expr.as_ref() {
                i.sym.to_string()
            } else {
                "_expr".to_string()
            }
        }
        SimpleAssignTarget::TsSatisfies(s) => {
            if let Expr::Ident(i) = s.expr.as_ref() {
                i.sym.to_string()
            } else {
                "_expr".to_string()
            }
        }
        SimpleAssignTarget::TsTypeAssertion(ta) => {
            if let Expr::Ident(i) = ta.expr.as_ref() {
                i.sym.to_string()
            } else {
                "_expr".to_string()
            }
        }
        SimpleAssignTarget::TsInstantiation(inst) => {
            if let Expr::Ident(i) = inst.expr.as_ref() {
                i.sym.to_string()
            } else {
                "_expr".to_string()
            }
        }
        SimpleAssignTarget::Invalid(_) => "_invalid".to_string(),
        SimpleAssignTarget::OptChain(opt) => match opt.base.as_ref() {
            OptChainBase::Member(m) => {
                if let Expr::Ident(i) = m.obj.as_ref() {
                    i.sym.to_string()
                } else {
                    "_opt".to_string()
                }
            }
            OptChainBase::Call(_) => "_opt_call".to_string(),
        },
    }
}

fn prop_name_to_string(key: &PropName) -> String {
    match key {
        PropName::Ident(ident_name) => ident_name.sym.to_string(),
        PropName::Str(s) => wtf8_str(&s.value).to_string(),
        PropName::Num(n) => n.value.to_string(),
        PropName::Computed(_) => "[]".to_string(),
        PropName::BigInt(bi) => bi.value.to_string(),
    }
}

/// If the assign target is a subscript (arr[i] = val), returns (obj_expr, idx_expr).
fn assign_target_subscript_parts(target: &AssignTarget) -> Option<(&Expr, &Expr)> {
    match target {
        AssignTarget::Simple(SimpleAssignTarget::Member(member)) => {
            match &member.prop {
                MemberProp::Computed(computed) => Some((&member.obj, &computed.expr)),
                _ => None,
            }
        }
        _ => None,
    }
}
