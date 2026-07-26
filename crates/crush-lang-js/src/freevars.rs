//! Free-variable analysis for `@javascript { ... }` polyglot blocks (CRUSH-68).
//!
//! Mirrors `crush_lang_python::analyzer::free_variables`: names the block
//! *reads* but never binds are inputs; top-level plain assignments are
//! candidate outputs (last one wins).

use std::collections::HashSet;

use swc_ecma_ast::*;

use crate::backend;

/// Result of free-variable analysis on a polyglot JS block.
pub struct FreeVars {
    /// Names read but never bound in this block (first-occurrence order).
    pub reads: Vec<String>,
    /// Names assigned at the block's own top level (occurrence order).
    /// Marshaling protocol picks the last entry as the single output.
    pub top_level_bound: Vec<String>,
}

/// Analyze a JS source block via swc — not a regex.
pub fn free_variables(source: &str) -> Result<FreeVars, String> {
    let module = backend::parse(source, "js").map_err(|e| e.to_string())?;

    let mut bound_anywhere: HashSet<String> = HashSet::new();
    for item in &module.body {
        collect_bound_item(item, &mut bound_anywhere);
    }

    let mut reads_seen: Vec<String> = Vec::new();
    let mut reads_dedup: HashSet<String> = HashSet::new();
    for item in &module.body {
        collect_reads_item(item, &mut reads_seen, &mut reads_dedup);
    }
    let reads = reads_seen
        .into_iter()
        .filter(|n| !bound_anywhere.contains(n))
        .collect();

    let mut top_level_bound = Vec::new();
    for item in &module.body {
        match item {
            ModuleItem::Stmt(Stmt::Decl(Decl::Var(var_decl))) => {
                for d in &var_decl.decls {
                    if let Some(name) = pat_ident_name(&d.name) {
                        top_level_bound.push(name);
                    }
                }
            }
            ModuleItem::Stmt(Stmt::Expr(ExprStmt { expr, .. })) => {
                if let Expr::Assign(AssignExpr {
                    left: AssignTarget::Simple(SimpleAssignTarget::Ident(id)),
                    ..
                }) = expr.as_ref()
                {
                    top_level_bound.push(id.id.sym.to_string());
                }
            }
            _ => {}
        }
    }

    Ok(FreeVars {
        reads,
        top_level_bound,
    })
}

fn pat_ident_name(pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(id) => Some(id.id.sym.to_string()),
        _ => None,
    }
}

fn collect_bound_item(item: &ModuleItem, bound: &mut HashSet<String>) {
    match item {
        ModuleItem::Stmt(stmt) => collect_bound_stmt(stmt, bound),
        ModuleItem::ModuleDecl(ModuleDecl::Import(import)) => {
            for spec in &import.specifiers {
                match spec {
                    ImportSpecifier::Named(n) => {
                        bound.insert(n.local.sym.to_string());
                    }
                    ImportSpecifier::Default(d) => {
                        bound.insert(d.local.sym.to_string());
                    }
                    ImportSpecifier::Namespace(n) => {
                        bound.insert(n.local.sym.to_string());
                    }
                }
            }
        }
        ModuleItem::ModuleDecl(_) => {}
    }
}

fn collect_bound_stmt(stmt: &Stmt, bound: &mut HashSet<String>) {
    match stmt {
        Stmt::Decl(Decl::Var(var_decl)) => {
            for d in &var_decl.decls {
                if let Some(name) = pat_ident_name(&d.name) {
                    bound.insert(name);
                }
            }
        }
        Stmt::Decl(Decl::Fn(f)) => {
            bound.insert(f.ident.sym.to_string());
        }
        Stmt::Decl(Decl::Class(c)) => {
            bound.insert(c.ident.sym.to_string());
        }
        Stmt::For(ForStmt { init, body, .. }) => {
            if let Some(VarDeclOrExpr::VarDecl(var_decl)) = init {
                for d in &var_decl.decls {
                    if let Some(name) = pat_ident_name(&d.name) {
                        bound.insert(name);
                    }
                }
            }
            collect_bound_stmt(body, bound);
        }
        Stmt::ForIn(ForInStmt { left, body, .. }) | Stmt::ForOf(ForOfStmt { left, body, .. }) => {
            if let ForHead::VarDecl(var_decl) = left {
                for d in &var_decl.decls {
                    if let Some(name) = pat_ident_name(&d.name) {
                        bound.insert(name);
                    }
                }
            }
            collect_bound_stmt(body, bound);
        }
        Stmt::While(WhileStmt { body, .. }) | Stmt::DoWhile(DoWhileStmt { body, .. }) => {
            collect_bound_stmt(body, bound);
        }
        Stmt::If(IfStmt {
            cons, alt, ..
        }) => {
            collect_bound_stmt(cons, bound);
            if let Some(alt) = alt {
                collect_bound_stmt(alt, bound);
            }
        }
        Stmt::Block(BlockStmt { stmts, .. }) => {
            for s in stmts {
                collect_bound_stmt(s, bound);
            }
        }
        Stmt::Try(try_stmt) => {
            for s in &try_stmt.block.stmts {
                collect_bound_stmt(s, bound);
            }
            if let Some(h) = &try_stmt.handler {
                if let Some(Pat::Ident(id)) = &h.param {
                    bound.insert(id.id.sym.to_string());
                }
                for s in &h.body.stmts {
                    collect_bound_stmt(s, bound);
                }
            }
            if let Some(f) = &try_stmt.finalizer {
                for s in &f.stmts {
                    collect_bound_stmt(s, bound);
                }
            }
        }
        _ => {}
    }
}

fn collect_reads_item(
    item: &ModuleItem,
    reads: &mut Vec<String>,
    dedup: &mut HashSet<String>,
) {
    match item {
        ModuleItem::Stmt(stmt) => collect_reads_stmt(stmt, reads, dedup),
        ModuleItem::ModuleDecl(_) => {}
    }
}

fn collect_reads_stmt(stmt: &Stmt, reads: &mut Vec<String>, dedup: &mut HashSet<String>) {
    match stmt {
        Stmt::Expr(ExprStmt { expr, .. }) => collect_reads_expr(expr, reads, dedup),
        Stmt::Decl(Decl::Var(var_decl)) => {
            for d in &var_decl.decls {
                if let Some(init) = &d.init {
                    collect_reads_expr(init, reads, dedup);
                }
            }
        }
        Stmt::Decl(Decl::Fn(f)) => {
            if let Some(body) = &f.function.body {
                for s in &body.stmts {
                    collect_reads_stmt(s, reads, dedup);
                }
            }
        }
        Stmt::Return(ReturnStmt { arg: Some(e), .. }) => collect_reads_expr(e, reads, dedup),
        Stmt::If(IfStmt {
            test,
            cons,
            alt,
            ..
        }) => {
            collect_reads_expr(test, reads, dedup);
            collect_reads_stmt(cons, reads, dedup);
            if let Some(a) = alt {
                collect_reads_stmt(a, reads, dedup);
            }
        }
        Stmt::While(WhileStmt { test, body, .. }) => {
            collect_reads_expr(test, reads, dedup);
            collect_reads_stmt(body, reads, dedup);
        }
        Stmt::Block(BlockStmt { stmts, .. }) => {
            for s in stmts {
                collect_reads_stmt(s, reads, dedup);
            }
        }
        Stmt::For(ForStmt {
            init,
            test,
            update,
            body,
            ..
        }) => {
            if let Some(VarDeclOrExpr::Expr(e)) = init {
                collect_reads_expr(e, reads, dedup);
            }
            if let Some(VarDeclOrExpr::VarDecl(v)) = init {
                for d in &v.decls {
                    if let Some(init) = &d.init {
                        collect_reads_expr(init, reads, dedup);
                    }
                }
            }
            if let Some(t) = test {
                collect_reads_expr(t, reads, dedup);
            }
            if let Some(u) = update {
                collect_reads_expr(u, reads, dedup);
            }
            collect_reads_stmt(body, reads, dedup);
        }
        _ => {}
    }
}

fn collect_reads_expr(expr: &Expr, reads: &mut Vec<String>, dedup: &mut HashSet<String>) {
    match expr {
        Expr::Ident(id) => {
            let name = id.sym.to_string();
            if dedup.insert(name.clone()) {
                reads.push(name);
            }
        }
        Expr::Bin(BinExpr { left, right, .. }) => {
            collect_reads_expr(left, reads, dedup);
            collect_reads_expr(right, reads, dedup);
        }
        Expr::Unary(UnaryExpr { arg, .. }) => collect_reads_expr(arg, reads, dedup),
        Expr::Assign(AssignExpr { right, left, .. }) => {
            collect_reads_expr(right, reads, dedup);
            // LHS ident is a bind, not a read — skip SimpleAssignTarget::Ident
            if let AssignTarget::Simple(SimpleAssignTarget::Member(m)) = left {
                collect_reads_expr(&m.obj, reads, dedup);
                if let MemberProp::Computed(c) = &m.prop {
                    collect_reads_expr(&c.expr, reads, dedup);
                }
            }
        }
        Expr::Call(CallExpr { callee, args, .. }) => {
            if let Callee::Expr(e) = callee {
                collect_reads_expr(e, reads, dedup);
            }
            for a in args {
                collect_reads_expr(&a.expr, reads, dedup);
            }
        }
        Expr::Member(MemberExpr { obj, prop, .. }) => {
            collect_reads_expr(obj, reads, dedup);
            if let MemberProp::Computed(c) = prop {
                collect_reads_expr(&c.expr, reads, dedup);
            }
        }
        Expr::Array(ArrayLit { elems, .. }) => {
            for e in elems.iter().flatten() {
                collect_reads_expr(&e.expr, reads, dedup);
            }
        }
        Expr::Object(ObjectLit { props, .. }) => {
            for p in props {
                if let PropOrSpread::Prop(prop) = p
                    && let Prop::KeyValue(kv) = prop.as_ref()
                {
                    collect_reads_expr(&kv.value, reads, dedup);
                }
            }
        }
        Expr::Paren(ParenExpr { expr, .. }) => collect_reads_expr(expr, reads, dedup),
        Expr::Tpl(Tpl { exprs, .. }) => {
            for e in exprs {
                collect_reads_expr(e, reads, dedup);
            }
        }
        Expr::Cond(CondExpr {
            test,
            cons,
            alt,
            ..
        }) => {
            collect_reads_expr(test, reads, dedup);
            collect_reads_expr(cons, reads, dedup);
            collect_reads_expr(alt, reads, dedup);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_read_and_top_level_assign() {
        let fv = free_variables("result = base * 2;\n").unwrap();
        assert_eq!(fv.reads, vec!["base".to_string()]);
        assert_eq!(fv.top_level_bound, vec!["result".to_string()]);
    }

    #[test]
    fn let_binding_is_output_not_free_read() {
        let fv = free_variables("let result = base + 1;\n").unwrap();
        assert_eq!(fv.reads, vec!["base".to_string()]);
        assert_eq!(fv.top_level_bound, vec!["result".to_string()]);
    }

    #[test]
    fn local_binding_not_free() {
        let fv = free_variables("let x = 1;\nlet result = x + 1;\n").unwrap();
        assert!(fv.reads.is_empty());
        assert_eq!(
            fv.top_level_bound,
            vec!["x".to_string(), "result".to_string()]
        );
    }
}
