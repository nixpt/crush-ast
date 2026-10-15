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
                // Vec, not Option: `export let a, b` / `export { a, b }` must emit
                // one Export per name. The old Option path returned after the first
                // declarator (clippy::never_loop), silently dropping the rest.
                main_body.extend(lower_module_decl(module_decl, ctx)?);
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

fn lower_module_decl(decl: &ModuleDecl, ctx: &LowerCtx) -> anyhow::Result<Vec<Statement>> {
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
            Ok(vec![Statement::Import {
                import: ImportStatement::CrushModule {
                    module_path: src,
                    alias: None,
                    selective,
                },
                meta: meta(&import.span, ctx),
            }])
        }
        ModuleDecl::ExportDecl(export) => match &export.decl {
            Decl::Fn(fn_decl) => {
                let name = fn_decl.ident.sym.to_string();
                Ok(vec![Statement::Export {
                    name: name.clone(),
                    value: Expression::Var { name, meta: meta(&fn_decl.function.span, ctx) },
                    meta: meta(&export.span, ctx),
                }])
            }
            Decl::Var(var_decl) => {
                // One Export per declarator — `export let a, b` must export both.
                let mut out = Vec::with_capacity(var_decl.decls.len());
                for d in &var_decl.decls {
                    let name = pat_to_name(&d.name);
                    out.push(Statement::Export {
                        name: name.clone(),
                        value: Expression::Var { name, meta: meta(&d.span, ctx) },
                        meta: meta(&export.span, ctx),
                    });
                }
                Ok(out)
            }
            Decl::Class(class) => {
                let name = class.ident.sym.to_string();
                Ok(vec![Statement::Export {
                    name: name.clone(),
                    value: Expression::Var { name, meta: meta(&class.class.span, ctx) },
                    meta: meta(&export.span, ctx),
                }])
            }
            Decl::Using(using) => {
                let mut out = Vec::with_capacity(using.decls.len());
                for d in &using.decls {
                    let name = pat_to_name(&d.name);
                    out.push(Statement::Export {
                        name,
                        value: Expression::NullLiteral { meta: meta(&d.span, ctx) },
                        meta: meta(&export.span, ctx),
                    });
                }
                Ok(out)
            }
            _ => Ok(vec![Statement::LangBlock {
                lang: "javascript".to_string(),
                code: "// export — not lowered".to_string(),
                variables: vec![],
                imports: vec![],
                deps: vec![],
                meta: meta(&export.span, ctx),
            }]),
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
                Ok(vec![Statement::Export {
                    name: "default".to_string(),
                    value: Expression::Lambda {
                        params,
                        body,
                        meta: meta(&fn_expr.function.span, ctx),
                    },
                    meta: meta(&export_default.span, ctx),
                }])
            }
            DefaultDecl::Class(class) => {
                let name = class
                    .ident
                    .as_ref()
                    .map(|id| id.sym.to_string())
                    .unwrap_or_default();
                Ok(vec![Statement::Export {
                    name: "default".to_string(),
                    value: Expression::Var { name, meta: meta(&class.class.span, ctx) },
                    meta: meta(&export_default.span, ctx),
                }])
            }
            _ => Ok(vec![Statement::LangBlock {
                lang: "javascript".to_string(),
                code: "// export default — not lowered".to_string(),
                variables: vec![],
                imports: vec![],
                deps: vec![],
                meta: meta(&export_default.span, ctx),
            }]),
        },
        ModuleDecl::ExportDefaultExpr(expr) => {
            let val = lower_expr(&expr.expr, ctx)?;
            Ok(vec![Statement::Export {
                name: "default".to_string(),
                value: val,
                meta: meta(&expr.span, ctx),
            }])
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
                Ok(vec![Statement::Import {
                    import: ImportStatement::CrushModule {
                        module_path: module.clone(),
                        alias: None,
                        selective,
                    },
                    meta: meta(&named.span, ctx),
                }])
            } else {
                // One Export per named specifier — `export { a, b }` must export both.
                let mut out = Vec::new();
                for spec in &named.specifiers {
                    if let ExportSpecifier::Named(ns) = spec {
                        let name = ns
                            .exported
                            .as_ref()
                            .map(|m| m.atom().to_string())
                            .unwrap_or_else(|| ns.orig.atom().to_string());
                        let orig = ns.orig.atom().to_string();
                        out.push(Statement::Export {
                            name,
                            value: Expression::Var {
                                name: orig,
                                meta: meta(&named.span, ctx),
                            },
                            meta: meta(&named.span, ctx),
                        });
                    }
                }
                Ok(out)
            }
        }
        ModuleDecl::ExportAll(all) => {
            let src = wtf8_str(&all.src.value).to_string();
            Ok(vec![Statement::Import {
                import: ImportStatement::CrushModule {
                    module_path: src,
                    alias: None,
                    selective: vec![],
                },
                meta: meta(&all.span, ctx),
            }])
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
            Ok(vec![Statement::LangBlock {
                lang: "javascript".to_string(),
                code: format!("// ts import equals: {}", mod_name),
                variables: vec![],
                imports: vec![],
                deps: vec![],
                meta: meta(&ts.span, ctx),
            }])
        }
        ModuleDecl::TsExportAssignment(ts) => {
            let val = lower_expr(&ts.expr, ctx)?;
            Ok(vec![Statement::Export {
                name: "default".to_string(),
                value: val,
                meta: meta(&ts.span, ctx),
            }])
        }
        ModuleDecl::TsNamespaceExport(ts) => Ok(vec![Statement::LangBlock {
            lang: "javascript".to_string(),
            code: format!("// ts namespace export: {}", ts.id.sym),
            variables: vec![],
            imports: vec![],
            deps: vec![],
            meta: meta(&ts.span, ctx),
        }]),
    }
}

fn lower_stmt(stmt: &Stmt, ctx: &LowerCtx) -> anyhow::Result<Option<Statement>> {
    Ok(Some(match stmt {
        Stmt::Expr(ExprStmt { expr, span, .. }) => {
            let mut stmts = expr_to_stmts(expr, span, ctx)?;
            if stmts.len() == 1 {
                stmts.pop().unwrap()
            } else {
                Statement::If {
                    condition: Expression::BoolLiteral { value: true, meta: meta(span, ctx) },
                    then_body: stmts,
                    else_body: None,
                    meta: meta(span, ctx),
                }
            }
        }
        Stmt::Decl(decl) => match decl {
            Decl::Fn(fn_decl) => lower_fn_decl(fn_decl, ctx)?,
            Decl::Var(var_decl) => {
                let mut stmts = Vec::new();
                for d in &var_decl.decls {
                    stmts.extend(lower_var_declarator(d, ctx)?);
                }
                match stmts.len() {
                    0 => return Ok(None),
                    1 => stmts.into_iter().next().unwrap(),
                    // `var a = 1, b = 2;` — keep every declarator, not just the first.
                    _ => Statement::If {
                        condition: Expression::BoolLiteral { value: true, meta: meta(&var_decl.span, ctx) },
                        then_body: stmts,
                        else_body: None,
                        meta: meta(&var_decl.span, ctx),
                    },
                }
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
        // `do { body } while (test)` runs the body first:
        // `while (true) { body; if (!test) break; }`, and a `continue` checks the test too.
        Stmt::DoWhile(DoWhileStmt { test, body, span, .. }) => {
            let exit_check = Statement::If {
                condition: Expression::UnaryOp {
                    operator: "!".to_string(),
                    operand: Box::new(lower_expr(test, ctx)?),
                    meta: meta(span, ctx),
                },
                then_body: vec![Statement::Break { meta: meta(span, ctx) }],
                else_body: None,
                meta: meta(span, ctx),
            };
            let mut body = rewrite_continue(block_or_stmt_to_vec(body, ctx)?, &[exit_check.clone()]);
            body.push(exit_check);
            Statement::While {
                condition: Box::new(Expression::BoolLiteral { value: true, meta: meta(span, ctx) }),
                body,
                meta: meta(span, ctx),
            }
        }
        // `for (init; test; update) body` → `init; while (test) { body; update }`.
        // It used to iterate a variable hard-coded as `i` over `make_range()`
        // (a 0..100 default), dropping `init`: loops stopped after 100
        // iterations, other names read an uninitialised slot, steps were ignored.
        Stmt::For(ForStmt {
            init,
            test,
            update,
            body,
            span,
            ..
        }) => {
            let mut out = Vec::new();
            match init {
                Some(VarDeclOrExpr::VarDecl(var_decl)) => {
                    for d in &var_decl.decls {
                        out.extend(lower_var_declarator(d, ctx)?);
                    }
                }
                Some(VarDeclOrExpr::Expr(e)) => out.extend(expr_to_stmts(e, span, ctx)?),
                None => {}
            }
            let condition = match test {
                Some(t) => lower_expr(t, ctx)?,
                None => Expression::BoolLiteral { value: true, meta: meta(span, ctx) },
            };
            let update_stmts = match update {
                Some(u) => expr_to_stmts(u, span, ctx)?,
                None => Vec::new(),
            };
            let mut body_stmts = rewrite_continue(block_or_stmt_to_vec(body, ctx)?, &update_stmts);
            body_stmts.extend(update_stmts);
            out.push(Statement::While {
                condition: Box::new(condition),
                body: body_stmts,
                meta: meta(span, ctx),
            });
            // One statement: `if (true) { ... }` groups init and loop.
            Statement::If {
                condition: Expression::BoolLiteral { value: true, meta: meta(span, ctx) },
                then_body: out,
                else_body: None,
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
        // `switch`: pick the first matching case (or `default` if none
        // matches), then run every case body from there down until a
        // `break` — JS fall-through. Lowered as
        //   let __sw_start = <index>; while (true) { if (start <= 0) {..} ...; break; }
        // so `break` exits the switch. It used to build this chain and then
        // discard it, so every switch was silently skipped.
        Stmt::Switch(SwitchStmt {
            discriminant,
            cases,
            span,
            ..
        }) => {
            let sm = meta(span, ctx);
            let id = span.lo.0;
            let val = format!("__sw_val_{id}");
            let start = format!("__sw_start_{id}");
            let mut out = vec![Statement::VarDecl {
                name: val.clone(),
                value: lower_expr(discriminant, ctx)?,
                type_hint: CastType::Any,
                meta: sm.clone(),
            }];
            let int = |v: i64| Expression::IntLiteral { value: v, meta: sm.clone() };
            let default_idx = cases.iter().position(|c| c.test.is_none());
            // start = first matching case index; else default; else past the end.
            let mut chain: Option<Vec<Statement>> = None;
            let fallback = vec![Statement::ExprStmt {
                expr: Expression::Call {
                    function: "__crush_assign__".to_string(),
                    args: vec![
                        Expression::Var { name: start.clone(), meta: sm.clone() },
                        int(default_idx.unwrap_or(cases.len()) as i64),
                    ],
                    meta: sm.clone(),
                },
                meta: sm.clone(),
            }];
            for (i, case) in cases.iter().enumerate().rev() {
                let Some(test) = &case.test else { continue };
                let assign = Statement::ExprStmt {
                    expr: Expression::Call {
                        function: "__crush_assign__".to_string(),
                        args: vec![Expression::Var { name: start.clone(), meta: sm.clone() }, int(i as i64)],
                        meta: sm.clone(),
                    },
                    meta: sm.clone(),
                };
                chain = Some(vec![Statement::If {
                    condition: Expression::BinaryOp {
                        operator: "===".to_string(),
                        left: Box::new(Expression::Var { name: val.clone(), meta: sm.clone() }),
                        right: Box::new(lower_expr(test, ctx)?),
                        meta: sm.clone(),
                    },
                    then_body: vec![assign],
                    else_body: Some(chain.take().unwrap_or_else(|| fallback.clone())),
                    meta: sm.clone(),
                }]);
            }
            out.push(Statement::VarDecl { name: start.clone(), value: int(cases.len() as i64), type_hint: CastType::Any, meta: sm.clone() });
            out.extend(chain.unwrap_or(fallback));
            let mut body = Vec::new();
            for (i, case) in cases.iter().enumerate() {
                let stmts = stmts_to_vec(&case.cons, ctx)?;
                if contains_loop_continue(&stmts) {
                    anyhow::bail!("`continue` inside a `switch` is not supported yet");
                }
                body.push(Statement::If {
                    condition: Expression::BinaryOp {
                        operator: "<=".to_string(),
                        left: Box::new(Expression::Var { name: start.clone(), meta: sm.clone() }),
                        right: Box::new(int(i as i64)),
                        meta: sm.clone(),
                    },
                    then_body: stmts,
                    else_body: None,
                    meta: sm.clone(),
                });
            }
            body.push(Statement::Break { meta: sm.clone() });
            out.push(Statement::While {
                condition: Box::new(Expression::BoolLiteral { value: true, meta: sm.clone() }),
                body,
                meta: sm.clone(),
            });
            Statement::If {
                condition: Expression::BoolLiteral { value: true, meta: sm.clone() },
                then_body: out,
                else_body: None,
                meta: sm,
            }
        }
        Stmt::Break(bs) => Statement::Break { meta: meta(&bs.span, ctx) },
        Stmt::Continue(cs) => Statement::Continue { meta: meta(&cs.span, ctx) },
        Stmt::With(_) => anyhow::bail!("`with` statements are not supported"),
        Stmt::Labeled(LabeledStmt { body, .. }) => {
            return lower_stmt(body, ctx);
        }
        Stmt::Block(_) | Stmt::Debugger(_) | Stmt::Empty(_) => return Ok(None),
    }))
}

/// Does `stmts` contain a `continue` that belongs to the enclosing loop
/// (i.e. not inside a nested loop or function)?
fn contains_loop_continue(stmts: &[Statement]) -> bool {
    stmts.iter().any(|s| match s {
        Statement::Continue { .. } => true,
        Statement::If { then_body, else_body, .. } => {
            contains_loop_continue(then_body) || else_body.as_deref().is_some_and(contains_loop_continue)
        }
        Statement::TryCatch { body, handler, .. } => contains_loop_continue(body) || contains_loop_continue(handler),
        _ => false,
    })
}

/// An expression in statement position; `a, b` becomes two statements.
fn expr_to_stmts(e: &Expr, span: &swc_common::Span, ctx: &LowerCtx) -> anyhow::Result<Vec<Statement>> {
    match unparen(e) {
        Expr::Seq(SeqExpr { exprs, .. }) => {
            let mut out = Vec::new();
            for e in exprs {
                out.extend(expr_to_stmts(e.as_ref(), span, ctx)?);
            }
            Ok(out)
        }
        e => Ok(vec![Statement::ExprStmt { expr: lower_expr(e, ctx)?, meta: meta(span, ctx) }]),
    }
}

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

fn lower_var_declarator(decl: &VarDeclarator, ctx: &LowerCtx) -> anyhow::Result<Vec<Statement>> {
    // `const [a, b] = …` / `const {x} = …` used to bind a placeholder named
    // `_arr` / `_obj`, leaving a and b uninitialised.
    if !matches!(&decl.name, Pat::Ident(_)) {
        anyhow::bail!("destructuring declarations are not supported yet");
    }
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
            let numeric = is_js_number(left) && is_js_number(right);
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
            // JS semantics as `js.*` capabilities (crush-vm js_core); crush's
            // own operators are unchanged.
            match operator {
                "/" => return Ok(js_div(left, right, m)),
                "+" if !numeric => return Ok(js_cap("js.add", vec![left, right], m)),
                "==" => return Ok(js_cap("js.loose_eq", vec![left, right], m)),
                "!=" => {
                    return Ok(Expression::UnaryOp {
                        operator: "!".to_string(),
                        operand: Box::new(js_cap("js.loose_eq", vec![left, right], m.clone())),
                        meta: m,
                    })
                }
                // The deciding operand, not a bool (`0 || 'dflt'` is 'dflt');
                // the right side runs only when the left doesn't decide.
                "||" | "&&" => {
                    let function = if operator == "||" { "__crush_js_or__" } else { "__crush_js_and__" };
                    return Ok(Expression::Call { function: function.to_string(), args: vec![left, right], meta: m });
                }
                _ => {}
            }
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
            // `__crush_ternary__` is compiled lazily (jumps), so only the
            // taken branch runs. `__crush_ifexpr__` was an unknown helper:
            // crush-walk-run stubbed it to null and evaluated both branches,
            // so `n < 2 ? n : fib(n - 1) + …` recursed forever.
            Ok(Expression::Call {
                function: "__crush_ternary__".to_string(),
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
                MemberProp::Ident(ident_name) if is_length_read(ident_name.sym.as_ref(), obj) => {
                    Ok(length_of(target, m))
                }
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
            // Where the value goes: a variable, or an object field / array slot.
            // `o.n = v` used to assign a variable literally named "o.n".
            enum Target { Var(String), Slot(Expression, Expression) }
            let target = match left {
                AssignTarget::Simple(SimpleAssignTarget::Member(member)) => {
                    let obj = lower_expr(&member.obj, ctx)?;
                    let key = match &member.prop {
                        MemberProp::Ident(i) => Expression::StringLiteral { value: i.sym.to_string(), meta: m.clone() },
                        MemberProp::Computed(c) => lower_expr(&c.expr, ctx)?,
                        MemberProp::PrivateName(_) => anyhow::bail!("assignment to a private field is not supported yet"),
                    };
                    Target::Slot(obj, key)
                }
                AssignTarget::Simple(simple) => match simple_assign_target_ident(simple) {
                    Some(name) => Target::Var(name),
                    None => anyhow::bail!("this assignment target is not supported yet"),
                },
                AssignTarget::Pat(_) => anyhow::bail!("destructuring assignment is not supported yet"),
            };
            let current = || match &target {
                Target::Var(name) => Expression::Var { name: name.clone(), meta: m.clone() },
                Target::Slot(obj, key) => Expression::Index {
                    target: Box::new(obj.clone()),
                    index: Box::new(key.clone()),
                    meta: m.clone(),
                },
            };
            let value = match op {
                AssignOp::Assign => right,
                AssignOp::AndAssign | AssignOp::OrAssign | AssignOp::NullishAssign => {
                    anyhow::bail!("logical assignment (`&&=`, `||=`, `??=`) is not supported yet")
                }
                AssignOp::DivAssign => js_div(current(), right, m.clone()),
                AssignOp::AddAssign => js_cap("js.add", vec![current(), right], m.clone()),
                _ => Expression::BinaryOp {
                    operator: match op {
                        AssignOp::AddAssign => "+", AssignOp::SubAssign => "-",
                        AssignOp::MulAssign => "*", AssignOp::DivAssign => "/",
                        AssignOp::ModAssign => "%", AssignOp::LShiftAssign => "<<",
                        AssignOp::RShiftAssign => ">>", AssignOp::BitOrAssign => "|",
                        AssignOp::BitXorAssign => "^", AssignOp::BitAndAssign => "&",
                        AssignOp::ExpAssign => "**",
                        _ => unreachable!(),
                    }
                    .to_string(),
                    left: Box::new(current()),
                    right: Box::new(right),
                    meta: m.clone(),
                },
            };
            Ok(match target {
                Target::Var(name) => Expression::Call {
                    function: "__crush_assign__".to_string(),
                    args: vec![Expression::Var { name, meta: m.clone() }, value],
                    meta: m,
                },
                Target::Slot(obj, key) => Expression::Call {
                    function: "__crush_setindex__".to_string(),
                    args: vec![obj, key, value],
                    meta: m,
                },
            })
        }
        // `(a, b)` in value position: only the last value survives. Statement
        // positions are split by `expr_to_stmts`; here a side-effecting
        // earlier part would be silently lost, so refuse it.
        Expr::Seq(SeqExpr { exprs, .. }) => {
            let mut last = Expression::NullLiteral { meta: m.clone() };
            for (i, e) in exprs.iter().enumerate() {
                if i + 1 < exprs.len()
                    && matches!(unparen(e), Expr::Assign(_) | Expr::Update(_) | Expr::Call(_) | Expr::New(_) | Expr::Await(_))
                {
                    anyhow::bail!("comma expression with side effects in value position is not supported");
                }
                last = lower_expr(e, ctx)?;
            }
            Ok(last)
        }
        Expr::Tpl(Tpl { exprs, quasis, .. }) => {
            // The cooked text (`\n` is a newline), and each `${…}` through JS
            // ToString (`${1.5 * 4}` is `6`) via js.add onto the string so far.
            let mut result = Expression::StringLiteral {
                value: quasis.first().map(tpl_cooked).transpose()?.unwrap_or_default(),
                meta: m.clone(),
            };
            for (i, expr) in exprs.iter().enumerate() {
                let expr = lower_expr(expr, ctx)?;
                result = js_cap("js.add", vec![result, expr], m.clone());
                if let Some(quasi) = quasis.get(i + 1) {
                    let text = tpl_cooked(quasi)?;
                    if !text.is_empty() {
                        let text = Expression::StringLiteral { value: text, meta: m.clone() };
                        result = js_cap("js.add", vec![result, text], m.clone());
                    }
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
                    MemberProp::Ident(i) if is_length_read(i.sym.as_ref(), &member.obj) => {
                        Ok(length_of(target, m))
                    }
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

/// Map a JS-side `Math.*` name to the `math.*` builtin name every consumer
/// actually dispatches on (CRUSH-65).
///
/// The JS spelling is capitalized (`Math.floor`); `crush-frontend`'s builtin
/// table and `crush-lang-sdk`'s host caps are both lowercase (`math.floor`).
/// Passing the JS spelling straight through matched no arm at all and fell into
/// the generic dotted method-call path, which produced a **wrong number with no
/// error** — `docs/benchmarks/compute.js` printed 165 instead of 465.
///
/// This is the same translate-at-lowering-time shape `Array.isArray` →
/// `is_array` and `JSON.parse` → `json_parse` already use in `lower_call_expr`.
fn math_builtin(js_name: &str) -> String {
    debug_assert!(js_name.starts_with("Math."));
    format!("math.{}", &js_name["Math.".len()..])
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
                // Node separates console.log arguments with a space; io.print
                // concatenates, so `console.log(a, b)` printed `ab`.
                "console.log" | "console.info" | "console.warn" | "console.error" => {
                    // Numbers print the way JS prints them (`6`, not crush's `6.0`;
                    // `1e+21`): each argument goes through conv.text(_, "js").
                    let mut spaced = Vec::with_capacity(lowered_args.len() * 2);
                    for (i, a) in lowered_args.into_iter().enumerate() {
                        if i > 0 {
                            spaced.push(Expression::StringLiteral { value: " ".to_string(), meta: m.clone() });
                        }
                        spaced.push(Expression::CapabilityCall {
                            name: "conv.text".to_string(),
                            args: vec![a, Expression::StringLiteral { value: "js".to_string(), meta: m.clone() }],
                            meta: m.clone(),
                        });
                    }
                    Ok(Expression::CapabilityCall {
                        name: "io.print".to_string(),
                        args: spaced,
                        meta: m,
                    })
                }
                "fetch" => Ok(Expression::CapabilityCall {
                    name: "net.http_get".to_string(),
                    args: lowered_args,
                    meta: m,
                }),
                "xip.invoke" => Ok(Expression::CapabilityCall {
                    name: "xip.invoke".to_string(),
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
                // CRUSH-65: lower to the lowercase `math.*` name as a CAPABILITY
                // call — the same shape `fetch` → `net.http_get` uses above.
                //
                // Not a plain `Expression::Call`: crush-frontend's semantic pass
                // (semantics.rs:412-418) resolves a Call's name against
                // user-defined functions and variables only — it has no builtin
                // registry — so ANY dotted name is rejected as "Undefined function"
                // before the compiler's `math.floor`/`math.pow` opcode arms
                // (compiler.rs:1416+) are ever consulted. Those arms are
                // unreachable from this path; the capability route is the one that
                // actually executes. All eleven names below are registered host caps
                // (crush-lang-sdk/src/stdlib.rs), so callers need the `stdlib` caps
                // granted. `sin`/`cos`/`tan` added in CRUSH-69 — they were absent
                // from CRUSH-65's producer list and still silently miscompiled.
                "Math.abs" | "Math.floor" | "Math.ceil" | "Math.round" | "Math.sqrt"
                | "Math.pow" | "Math.min" | "Math.max"
                | "Math.sin" | "Math.cos" | "Math.tan" => Ok(Expression::CapabilityCall {
                    name: math_builtin(&func_name),
                    args: lowered_args,
                    meta: m,
                }),
                "Math.random" => Ok(Expression::CapabilityCall {
                    name: "math.random".to_string(),
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

/// The variable a simple assignment target names, looking through parens and
/// TS wrappers; `None` for anything that isn't a plain identifier.
fn simple_assign_target_ident(target: &SimpleAssignTarget) -> Option<String> {
    let inner = match target {
        SimpleAssignTarget::Ident(binding) => return Some(binding.id.sym.to_string()),
        SimpleAssignTarget::Paren(p) => &p.expr,
        SimpleAssignTarget::TsAs(t) => &t.expr,
        SimpleAssignTarget::TsNonNull(t) => &t.expr,
        SimpleAssignTarget::TsSatisfies(t) => &t.expr,
        _ => return None,
    };
    match unparen(inner) {
        Expr::Ident(i) => Some(i.sym.to_string()),
        _ => None,
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

/// CRUSH-260: `x.length` reads a string's or array's length, so it lowers to
/// the `len` op (native on every engine, incl. AOT) instead of a field read,
/// which yielded `null`. An object *literal* keeps the field read
/// (`({length: 3}).length`); a plain object in a variable with its own
/// `length` key isn't distinguishable here and gets `len`'s type error.
/// Strings measure in bytes (crush `len`, CRUSH-114), so `.length` matches JS
/// for ASCII but not for non-ASCII text (JS counts UTF-16 units).
fn is_length_read(field: &str, obj: &Expr) -> bool {
    field == "length" && !matches!(unparen(obj), Expr::Object(_))
}

/// JS `/` is real division (`7 / 2` is 3.5; crush's `/` on two ints
/// truncates). An exact quotient of two ints stays an int (`4 / 2` is `2`,
/// usable as an array index and printed `2` even inside string
/// concatenation) — JS has one number type, so that is safe. Division by
/// zero is a loud error rather than Infinity.
fn js_div(left: Expression, right: Expression, meta: HashMap<String, serde_json::Value>) -> Expression {
    Expression::CapabilityCall { name: "js.div".to_string(), args: vec![left, right], meta }
}

/// A call to a `js.*` capability: JS semantics implemented once in crush-vm
/// (`js_core`) and mirrored by the AOT runtimes.
fn js_cap(name: &str, args: Vec<Expression>, meta: HashMap<String, serde_json::Value>) -> Expression {
    Expression::CapabilityCall { name: name.to_string(), args, meta }
}

/// Whether JS evaluates `e` to a number whatever the variables hold: number
/// literals and arithmetic on them. Such a `+` stays crush's numeric `+`;
/// any other `+` may involve a string and goes through `js.add`.
fn is_js_number(e: &Expr) -> bool {
    match unparen(e) {
        Expr::Lit(Lit::Num(_)) => true,
        Expr::Unary(UnaryExpr { op: UnaryOp::Minus, arg, .. }) => is_js_number(arg),
        Expr::Bin(BinExpr { op, left, right, .. }) => {
            matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod | BinaryOp::Exp)
                && is_js_number(left)
                && is_js_number(right)
        }
        _ => false,
    }
}

/// A template literal part's cooked text (escapes applied).
fn tpl_cooked(q: &TplElement) -> anyhow::Result<String> {
    match &q.cooked {
        Some(text) => match text.as_str() {
            Some(text) => Ok(text.to_string()),
            None => anyhow::bail!("a template literal with a lone surrogate escape is not supported yet"),
        },
        None => anyhow::bail!("invalid escape sequence in template literal `{}`", q.raw),
    }
}

fn unparen(e: &Expr) -> &Expr {
    match e {
        Expr::Paren(ParenExpr { expr, .. }) => unparen(expr),
        other => other,
    }
}

fn length_of(target: Expression, meta: HashMap<String, serde_json::Value>) -> Expression {
    Expression::Call { function: "len".to_string(), args: vec![target], meta }
}
