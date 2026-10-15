//! Statements, declarations and function bodies.

use boa_ast::declaration::{
    Binding, Declaration, ExportDeclaration, ImportKind, LexicalDeclaration, VarDeclaration,
    Variable,
};
use std::collections::HashSet;
use std::convert::Infallible;
use std::ops::ControlFlow;

use boa_ast::expression::{Expression, Identifier};
use boa_ast::function::FunctionDeclaration;
use boa_ast::function::{FormalParameterList, FunctionBody};
use boa_ast::statement::LabelledItem;
use boa_ast::statement::iteration::{ForLoopInitializer, IterableLoopInitializer};
use boa_ast::visitor::{VisitWith, Visitor};
use boa_ast::{ModuleItem, Span, Spanned, Statement, StatementListItem};
use boa_interner::Sym;
use crush_cast::{
    CastType, Expression as CastExpr, Function, ImportStatement, Statement as CastStmt,
};

use super::scope::{self, lexical_syms, param_syms, var_syms};
use super::{Lower, Meta, group};
use crate::lower_common::{contains_loop_continue, for_in_guarded, rewrite_continue};

/// A top-level item of a script or a module.
pub(super) enum TopItem<'a> {
    Stmt(&'a StatementListItem),
    Module(&'a ModuleItem),
}

impl<'a> TopItem<'a> {
    fn stmt(&self) -> Option<&'a StatementListItem> {
        match self {
            TopItem::Stmt(s) => Some(s),
            TopItem::Module(ModuleItem::StatementListItem(s)) => Some(s),
            TopItem::Module(_) => None,
        }
    }
}

/// A function's parameters and body, lowered.
pub(super) type LoweredFn = (Vec<(String, CastType)>, Vec<CastStmt>);

/// The span a statement is reported at: its leading expression's, if any.
pub(super) fn stmt_span(s: &Statement) -> Option<Span> {
    match s {
        Statement::Expression(e) => Some(e.span()),
        Statement::If(i) => Some(i.cond().span()),
        Statement::WhileLoop(w) => Some(w.condition().span()),
        Statement::DoWhileLoop(w) => Some(w.cond().span()),
        Statement::ForLoop(f) => f
            .condition()
            .map(Spanned::span)
            .or_else(|| stmt_span(f.body())),
        Statement::ForInLoop(f) => Some(f.target().span()),
        Statement::ForOfLoop(f) => Some(f.iterable().span()),
        Statement::Return(r) => r.target().map(Spanned::span),
        Statement::Throw(t) => Some(t.target().span()),
        Statement::Switch(s) => Some(s.val().span()),
        Statement::Var(v) => {
            v.0.as_ref()
                .iter()
                .find_map(|d| d.init().map(Spanned::span))
        }
        Statement::Labelled(l) => match l.item() {
            LabelledItem::Statement(s) => stmt_span(s),
            LabelledItem::FunctionDeclaration(f) => Some(f.name().span()),
        },
        Statement::Block(b) => items_span(b.statement_list().statements()),
        _ => None,
    }
}

fn items_span(items: &[StatementListItem]) -> Option<Span> {
    items.iter().find_map(|i| match i {
        StatementListItem::Statement(s) => stmt_span(s),
        StatementListItem::Declaration(_) => None,
    })
}

impl Lower<'_> {
    // ── Top level ──────────────────────────────────────────────────────────

    /// The script's (or module's) body, which becomes `main`. Top-level
    /// `function` declarations become crush functions of their own, as in the
    /// swc lowering; they can't see `main`'s locals.
    pub(super) fn top_level(&mut self, items: &[TopItem<'_>]) -> anyhow::Result<Vec<CastStmt>> {
        let list: Vec<&StatementListItem> = items.iter().filter_map(TopItem::stmt).collect();

        self.scopes.enter_fn();
        let mut syms = Vec::new();
        for item in &list {
            var_syms(std::slice::from_ref(*item), &mut syms);
            lexical_syms(std::slice::from_ref(*item), &mut syms);
        }
        for item in items {
            if let TopItem::Module(ModuleItem::ImportDeclaration(import)) = item {
                if let Some(d) = import.default() {
                    syms.push(d.sym());
                }
                match import.kind() {
                    ImportKind::DefaultOrUnnamed => {}
                    ImportKind::Namespaced { binding } => syms.push(binding.sym()),
                    ImportKind::Named { names } => {
                        syms.extend(names.iter().map(|n| n.binding().sym()))
                    }
                }
            }
        }
        for sym in syms {
            let name = self.name(sym);
            self.scopes.declare_root(sym, name);
        }
        for item in &list {
            if let StatementListItem::Declaration(d) = item
                && let Declaration::FunctionDeclaration(f) = d.as_ref()
            {
                self.toplevel_fns.insert(f.name().sym());
            }
        }

        let mut body = Vec::new();
        for item in &list {
            body.extend(self.uninit_vars(std::slice::from_ref(*item)));
        }
        for item in items {
            match item {
                TopItem::Module(ModuleItem::ImportDeclaration(import)) => {
                    body.push(self.import(import))
                }
                TopItem::Module(ModuleItem::ExportDeclaration(e)) => body.extend(self.export(e)?),
                _ => {
                    let s = item.stmt().expect("a statement");
                    if let StatementListItem::Declaration(d) = s
                        && let Declaration::FunctionDeclaration(f) = d.as_ref()
                    {
                        let name = self.name(f.name().sym());
                        if name == "main" {
                            // `main` is the crush function the script body becomes.
                            return Err(self.unsupported(
                                "a top-level function named `main`",
                                Some(f.name().span()),
                            ));
                        }
                        let saved = self.scopes.isolate();
                        let lowered =
                            self.function(f.parameters(), f.body(), Some(f.name().span()));
                        self.scopes.restore(saved);
                        let (params, fbody) = lowered?;
                        self.functions.insert(
                            name,
                            Function {
                                params,
                                body: fbody,
                                ..Default::default()
                            },
                        );
                    } else {
                        body.extend(self.item(s)?);
                    }
                }
            }
        }
        self.scopes.leave_fn();
        Ok(body)
    }

    fn import(&self, import: &boa_ast::declaration::ImportDeclaration) -> CastStmt {
        let mut selective = Vec::new();
        if let Some(d) = import.default() {
            selective.push(self.name(d.sym()));
        }
        match import.kind() {
            ImportKind::DefaultOrUnnamed => {}
            ImportKind::Namespaced { binding } => selective.push(self.name(binding.sym())),
            ImportKind::Named { names } => {
                selective.extend(names.iter().map(|n| self.name(n.binding().sym())))
            }
        }
        CastStmt::Import {
            import: ImportStatement::CrushModule {
                module_path: self.name(import.specifier().sym()),
                alias: None,
                selective,
            },
            meta: self.meta(None),
        }
    }

    fn export(&mut self, e: &ExportDeclaration) -> anyhow::Result<Vec<CastStmt>> {
        let export = |this: &Self, name: String, value: CastExpr| CastStmt::Export {
            name,
            value,
            meta: this.meta(None),
        };
        let var = |this: &Self, name: String| CastExpr::Var {
            name,
            meta: this.meta(None),
        };
        match e {
            ExportDeclaration::List(specs) => Ok(specs
                .iter()
                .map(|s| {
                    let local = self.resolve(s.private_name());
                    export(self, self.name(s.alias()), var(self, local))
                })
                .collect()),
            ExportDeclaration::VarStatement(v) => {
                let mut out = self.var_decl(v)?;
                for d in v.0.as_ref() {
                    let name = self.binding_name(d.binding())?;
                    out.push(export(self, name.clone(), var(self, name)));
                }
                Ok(out)
            }
            ExportDeclaration::Declaration(Declaration::Lexical(lex)) => {
                let mut out = self.lexical(lex)?;
                for d in lex.variable_list().as_ref() {
                    let name = self.binding_name(d.binding())?;
                    out.push(export(self, name.clone(), var(self, name)));
                }
                Ok(out)
            }
            ExportDeclaration::DefaultAssignmentExpression(expr) => {
                let value = self.expr(expr)?;
                Ok(vec![export(self, "default".to_string(), value)])
            }
            ExportDeclaration::DefaultFunctionDeclaration(f) => {
                let value = self.lambda(f.parameters(), f.body(), Some(f.name().span()))?;
                Ok(vec![export(self, "default".to_string(), value)])
            }
            ExportDeclaration::ReExport { .. } => {
                Err(self.unsupported("a re-export (`export … from …`)", None))
            }
            ExportDeclaration::Declaration(_)
            | ExportDeclaration::DefaultGeneratorDeclaration(_)
            | ExportDeclaration::DefaultAsyncFunctionDeclaration(_)
            | ExportDeclaration::DefaultAsyncGeneratorDeclaration(_)
            | ExportDeclaration::DefaultClassDeclaration(_) => Err(self.unsupported(
                "exporting a function, class, generator or async declaration",
                None,
            )),
        }
    }

    // ── Functions ───────────────────────────────────────────────────────────

    /// A function's parameters and body, in a new crush function scope. The
    /// caller decides whether it sees the enclosing locals (a lambda) or not
    /// (a top-level function: [`super::scope::Scopes::isolate`]).
    pub(super) fn function(
        &mut self,
        params: &FormalParameterList,
        body: &FunctionBody,
        span: Option<Span>,
    ) -> anyhow::Result<LoweredFn> {
        self.scopes.enter_fn();
        let r = self.function_in_scope(params, body, span);
        self.scopes.leave_fn();
        r
    }

    fn function_in_scope(
        &mut self,
        params: &FormalParameterList,
        body: &FunctionBody,
        span: Option<Span>,
    ) -> anyhow::Result<LoweredFn> {
        let mut lowered_params = Vec::new();
        for p in params.as_ref() {
            if p.is_rest_param() {
                return Err(self.unsupported("a rest parameter (`...args`)", span));
            }
            if let Some(init) = p.init() {
                return Err(
                    self.unsupported("a default parameter value (`x = …`)", Some(init.span()))
                );
            }
            let Binding::Identifier(id) = p.variable().binding() else {
                return Err(self.unsupported("a destructuring parameter", span));
            };
            let name = self.name(id.sym());
            self.scopes.declare_root(id.sym(), name.clone());
            lowered_params.push((name, CastType::Any));
        }
        let items = body.statements();
        let mut syms = Vec::new();
        param_syms(params, &mut syms);
        var_syms(items, &mut syms);
        lexical_syms(items, &mut syms);
        for sym in syms {
            let name = self.name(sym);
            self.scopes.declare_root(sym, name);
        }
        let mut out = self.uninit_vars(items);
        out.extend(self.hoisted_functions(items, false)?);
        out.extend(self.items(items)?);
        Ok((lowered_params, out))
    }

    /// `function f() {}` inside a function or block, before the other
    /// statements of its scope run (hoisting):
    /// - one that uses no binding of an enclosing function is **lifted** to a
    ///   crush function of its own under a fresh name (calls are direct, as
    ///   for a top-level function, so AOT can compile them);
    /// - one that does is a closure bound to `f`.
    fn hoisted_functions(
        &mut self,
        items: &[StatementListItem],
        block: bool,
    ) -> anyhow::Result<Vec<CastStmt>> {
        let decls: Vec<&FunctionDeclaration> = items
            .iter()
            .filter_map(|item| match item {
                StatementListItem::Declaration(d) => match d.as_ref() {
                    Declaration::FunctionDeclaration(f) => Some(f),
                    _ => None,
                },
                StatementListItem::Statement(_) => None,
            })
            .collect();
        if decls.is_empty() {
            return Ok(vec![]);
        }
        // Which ones capture nothing? A use of a sibling that is lifted too
        // isn't a capture, so iterate to a fixpoint.
        let idents: Vec<HashSet<Sym>> = decls.iter().map(|f| idents_of(f)).collect();
        let names: Vec<Sym> = decls.iter().map(|f| f.name().sym()).collect();
        let mut lift = vec![true; decls.len()];
        loop {
            let mut changed = false;
            for i in 0..decls.len() {
                if !lift[i] {
                    continue;
                }
                let captures = idents[i].iter().any(|s| {
                    let sibling = names.iter().zip(&lift).any(|(n, l)| n == s && *l);
                    !sibling && self.is_enclosing_binding(*s)
                });
                if captures {
                    lift[i] = false;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        // Rebind the lifted names before lowering anything that uses them.
        let mut lifted = Vec::new();
        for (i, f) in decls.iter().enumerate() {
            if lift[i] {
                let global = self.fresh_name(f.name().sym());
                self.scopes.declare_block(f.name().sym(), global.clone());
                lifted.push((f.name().sym(), global));
            }
        }
        let mut out = Vec::new();
        for (i, f) in decls.iter().enumerate() {
            let span = Some(f.name().span());
            if lift[i] {
                let saved = self.scopes.isolate();
                self.scopes.enter_fn();
                for (sym, global) in &lifted {
                    self.scopes.declare_root(*sym, global.clone());
                }
                let lowered = self.function(f.parameters(), f.body(), span);
                self.scopes.restore(saved);
                let (params, body) = lowered?;
                let global = self.resolve(f.name().sym());
                self.functions.insert(
                    global,
                    Function {
                        params,
                        body,
                        ..Default::default()
                    },
                );
            } else {
                let name = self.resolve(f.name().sym());
                let value = self.lambda(f.parameters(), f.body(), span)?;
                out.push(self.var_decl_stmt(name, value, block, span));
            }
        }
        Ok(out)
    }

    /// Is `sym` a binding of an enclosing function (a capture), as opposed to
    /// a global or a top-level function?
    fn is_enclosing_binding(&self, sym: Sym) -> bool {
        self.scopes.resolve(sym).is_some() && !self.toplevel_fns.contains(&sym)
    }

    // ── Statement lists and blocks ──────────────────────────────────────────

    /// A statement list in the current scope.
    pub(super) fn items(&mut self, items: &[StatementListItem]) -> anyhow::Result<Vec<CastStmt>> {
        let mut out = Vec::new();
        for item in items {
            out.extend(self.item(item)?);
        }
        Ok(out)
    }

    fn item(&mut self, item: &StatementListItem) -> anyhow::Result<Vec<CastStmt>> {
        match item {
            StatementListItem::Statement(s) => self.stmt(s),
            StatementListItem::Declaration(d) => self.decl(d),
        }
    }

    /// Declare a block's bindings under fresh names.
    fn declare_block_syms(&mut self, syms: Vec<boa_interner::Sym>) {
        for sym in syms {
            let fresh = self.fresh_name(sym);
            self.scopes.declare_block(sym, fresh);
        }
    }

    /// A name for a block-level binding that nothing else uses.
    pub(super) fn fresh_name(&mut self, sym: Sym) -> String {
        let base = self.name(sym);
        self.hidden(&format!("{base}__"))
    }

    /// A `{ … }` block: its own scope; the statements are flattened into the
    /// enclosing list (the compiler doesn't scope blocks; the names do).
    fn block(&mut self, items: &[StatementListItem]) -> anyhow::Result<Vec<CastStmt>> {
        self.scopes.push_block();
        let mut syms = Vec::new();
        lexical_syms(items, &mut syms);
        self.declare_block_syms(syms);
        let r = (|| {
            let mut out = self.hoisted_functions(items, true)?;
            out.extend(self.items(items)?);
            Ok(out)
        })();
        self.scopes.pop_block();
        r
    }

    /// An `if`/loop body: a block, or one statement.
    fn body(&mut self, s: &Statement) -> anyhow::Result<Vec<CastStmt>> {
        match s {
            Statement::Block(b) => self.block(b.statement_list().statements()),
            other => self.stmt(other),
        }
    }

    // ── Declarations ────────────────────────────────────────────────────────

    fn decl(&mut self, d: &Declaration) -> anyhow::Result<Vec<CastStmt>> {
        match d {
            Declaration::Lexical(lex) => self.lexical(lex),
            // Hoisted: lowered at the start of its scope.
            Declaration::FunctionDeclaration(_) => Ok(vec![]),
            Declaration::GeneratorDeclaration(f) => {
                Err(self.unsupported("a generator function (`function*`)", Some(f.name().span())))
            }
            Declaration::AsyncFunctionDeclaration(f) => {
                Err(self.unsupported("an async function", Some(f.name().span())))
            }
            Declaration::AsyncGeneratorDeclaration(f) => {
                Err(self.unsupported("an async generator", Some(f.name().span())))
            }
            Declaration::ClassDeclaration(_) => {
                Err(self.unsupported("a class (waits for the COSM object model)", None))
            }
        }
    }

    pub(super) fn binding_name(&self, b: &Binding) -> anyhow::Result<String> {
        match b {
            Binding::Identifier(id) => Ok(self.resolve(id.sym())),
            Binding::Pattern(_) => Err(self.unsupported("a destructuring pattern", None)),
        }
    }

    fn var_decl_stmt(
        &self,
        name: String,
        value: CastExpr,
        block: bool,
        span: Option<Span>,
    ) -> CastStmt {
        // `let` / `const` are block bindings: a fresh one each time the
        // declaration runs (CRUSH-251 step 4b); `var` is one function-scoped
        // binding.
        let mut meta = self.meta(span);
        if block {
            meta.insert("binding".to_string(), serde_json::json!("block"));
        }
        CastStmt::VarDecl {
            name,
            value,
            type_hint: CastType::Any,
            meta,
        }
    }

    fn variable(&mut self, v: &Variable, block: bool) -> anyhow::Result<Vec<CastStmt>> {
        let Binding::Identifier(id) = v.binding() else {
            return Err(
                self.unsupported("a destructuring declaration", v.init().map(Spanned::span))
            );
        };
        let name = self.resolve(id.sym());
        let span = v.init().map(Spanned::span);
        let value = match v.init() {
            Some(init) => self.expr(init)?,
            // `var x;` is hoisted (see `uninit_vars`): running it again, in a
            // loop or after an assignment, must not reset `x`.
            None if !block => return Ok(vec![]),
            None => CastExpr::NullLiteral {
                meta: self.meta(span),
            },
        };
        Ok(vec![self.var_decl_stmt(name, value, block, span)])
    }

    /// `x = null` at the start of a function for each `var x;` without an
    /// initializer in it, so the name exists however control reaches its use.
    fn uninit_vars(&mut self, items: &[StatementListItem]) -> Vec<CastStmt> {
        let mut syms = Vec::new();
        scope::uninit_var_syms(items, &mut syms);
        let mut seen = std::collections::HashSet::new();
        syms.into_iter()
            .filter(|s| seen.insert(*s))
            .map(|sym| {
                let name = self.resolve(sym);
                let value = CastExpr::NullLiteral {
                    meta: self.meta(None),
                };
                self.var_decl_stmt(name, value, false, None)
            })
            .collect()
    }

    fn var_decl(&mut self, v: &VarDeclaration) -> anyhow::Result<Vec<CastStmt>> {
        let mut out = Vec::new();
        for d in v.0.as_ref() {
            out.extend(self.variable(d, false)?);
        }
        Ok(out)
    }

    fn lexical(&mut self, lex: &LexicalDeclaration) -> anyhow::Result<Vec<CastStmt>> {
        let list = match lex {
            LexicalDeclaration::Let(l) | LexicalDeclaration::Const(l) => l,
            LexicalDeclaration::Using(_) => {
                return Err(self.unsupported("a `using` declaration", None));
            }
            LexicalDeclaration::AwaitUsing(_) => {
                return Err(self.unsupported("an `await using` declaration", None));
            }
        };
        let mut out = Vec::new();
        for d in list.as_ref() {
            out.extend(self.variable(d, true)?);
        }
        Ok(out)
    }

    // ── Statements ──────────────────────────────────────────────────────────

    pub(super) fn stmt(&mut self, s: &Statement) -> anyhow::Result<Vec<CastStmt>> {
        let span = stmt_span(s);
        let m = self.meta(span);
        Ok(match s {
            Statement::Block(b) => self.block(b.statement_list().statements())?,
            Statement::Var(v) => self.var_decl(v)?,
            Statement::Empty | Statement::Debugger => vec![],
            Statement::Expression(e) => self.expr_to_stmts(e)?,
            Statement::If(i) => {
                let condition = self.expr(i.cond())?;
                let then_body = self.body(i.body())?;
                let else_body = match i.else_node() {
                    Some(e) => Some(self.body(e)?),
                    None => None,
                };
                vec![CastStmt::If {
                    condition,
                    then_body,
                    else_body,
                    meta: m,
                }]
            }
            Statement::WhileLoop(w) => {
                let condition = self.expr(w.condition())?;
                let body = self.body(w.body())?;
                vec![CastStmt::While {
                    condition: Box::new(condition),
                    body,
                    meta: m,
                }]
            }
            // `do { body } while (test)` runs the body first:
            // `while (true) { body; if (!test) break; }`; a `continue` checks the test too.
            Statement::DoWhileLoop(dw) => {
                let exit_check = CastStmt::If {
                    condition: CastExpr::UnaryOp {
                        operator: "!".to_string(),
                        operand: Box::new(self.expr(dw.cond())?),
                        meta: m.clone(),
                    },
                    then_body: vec![CastStmt::Break { meta: m.clone() }],
                    else_body: None,
                    meta: m.clone(),
                };
                let mut body =
                    rewrite_continue(self.body(dw.body())?, std::slice::from_ref(&exit_check));
                body.push(exit_check);
                vec![CastStmt::While {
                    condition: Box::new(CastExpr::BoolLiteral {
                        value: true,
                        meta: m.clone(),
                    }),
                    body,
                    meta: m,
                }]
            }
            Statement::ForLoop(fl) => {
                // A `let` head is its own scope around the loop.
                let lexical = match fl.init() {
                    Some(ForLoopInitializer::Lexical(l)) => Some(l.declaration()),
                    _ => None,
                };
                if lexical.is_some() {
                    self.scopes.push_block();
                }
                let r = self.for_loop(fl, lexical, m);
                if lexical.is_some() {
                    self.scopes.pop_block();
                }
                r?
            }
            Statement::ForInLoop(fi) => {
                let is_object = matches!(unparen(fi.target()), Expression::ObjectLiteral(_));
                let iterable = self.expr(fi.target())?;
                let (variable, body, scoped) = self.for_head(fi.initializer(), fi.body(), span)?;
                let out = if is_object {
                    CastStmt::For {
                        variable,
                        iterable: Box::new(iterable),
                        body,
                        meta: m.clone(),
                    }
                } else {
                    let id = self.id();
                    for_in_guarded(variable, iterable, body, id, m.clone())
                };
                if scoped {
                    self.scopes.pop_block();
                }
                vec![out]
            }
            Statement::ForOfLoop(fo) => {
                if fo.r#await() {
                    return Err(self.unsupported("`for await`", span));
                }
                let iterable = self.expr(fo.iterable())?;
                let (variable, body, scoped) = self.for_head(fo.initializer(), fo.body(), span)?;
                if scoped {
                    self.scopes.pop_block();
                }
                vec![CastStmt::For {
                    variable,
                    iterable: Box::new(iterable),
                    body,
                    meta: m,
                }]
            }
            Statement::Switch(sw) => self.switch(sw, m)?,
            // A labeled jump would lose its label and act on the innermost loop.
            Statement::Break(b) if b.label().is_some() => {
                return Err(self.unsupported("a labeled `break` (`break label`)", span));
            }
            Statement::Continue(c) if c.label().is_some() => {
                return Err(self.unsupported("a labeled `continue` (`continue label`)", span));
            }
            Statement::Break(_) => vec![CastStmt::Break { meta: m }],
            Statement::Continue(_) => vec![CastStmt::Continue { meta: m }],
            Statement::Return(r) => {
                let value = match r.target() {
                    Some(e) => Some(self.expr(e)?),
                    None => None,
                };
                vec![CastStmt::Return { value, meta: m }]
            }
            Statement::Throw(t) => vec![CastStmt::Throw {
                value: self.expr(t.target())?,
                meta: m,
            }],
            Statement::Try(t) => {
                // `finally` must run on return and on throw.
                if t.finally().is_some() {
                    return Err(self.unsupported("`try … finally`", span));
                }
                let body = self.block(t.block().statement_list().statements())?;
                let (error_var, handler) = match t.catch() {
                    Some(c) => {
                        self.scopes.push_block();
                        let r = (|| {
                            let var = match c.parameter() {
                                Some(Binding::Identifier(id)) => {
                                    let fresh = self.fresh_name(id.sym());
                                    self.scopes.declare_block(id.sym(), fresh.clone());
                                    fresh
                                }
                                Some(Binding::Pattern(_)) => {
                                    return Err(
                                        self.unsupported("a destructuring `catch` parameter", span)
                                    );
                                }
                                None => self.hidden("__catch_"),
                            };
                            Ok((var, self.block(c.block().statement_list().statements())?))
                        })();
                        self.scopes.pop_block();
                        r?
                    }
                    None => (self.hidden("__catch_"), vec![]),
                };
                vec![CastStmt::TryCatch {
                    body,
                    error_var,
                    handler,
                    meta: m,
                }]
            }
            // A label nothing jumps to is harmless (labeled jumps are refused).
            Statement::Labelled(l) => match l.item() {
                LabelledItem::Statement(s) => self.stmt(s)?,
                LabelledItem::FunctionDeclaration(f) => {
                    return Err(
                        self.unsupported("a labeled function declaration", Some(f.name().span()))
                    );
                }
            },
            Statement::With(_) => return Err(self.unsupported("a `with` statement", span)),
        })
    }

    /// `for (init; test; update) body` → `init; while (test) { body; update }`.
    /// `for (let i ..)`: each iteration gets its own `i` (the spec's
    /// CreatePerIterationEnvironment), copied before the update so `i++`
    /// changes the new binding and a closure made in iteration k keeps k.
    /// `__crush_rebind__` does it (only if a closure captures `i`; otherwise
    /// it compiles to nothing). `var` shares one binding.
    fn for_loop(
        &mut self,
        fl: &boa_ast::statement::ForLoop,
        lexical: Option<&LexicalDeclaration>,
        m: Meta,
    ) -> anyhow::Result<Vec<CastStmt>> {
        let mut out = Vec::new();
        let mut per_iteration = Vec::new();
        match fl.init() {
            Some(ForLoopInitializer::Var(v)) => out.extend(self.var_decl(v)?),
            Some(ForLoopInitializer::Lexical(_)) => {
                let lex = lexical.expect("lexical head");
                let mut syms = Vec::new();
                scope::lexical_decl_syms(lex, &mut syms);
                self.declare_block_syms(syms.clone());
                out.extend(self.lexical(lex)?);
                per_iteration = syms.iter().map(|s| self.resolve(*s)).collect();
            }
            Some(ForLoopInitializer::Expression(e)) => out.extend(self.expr_to_stmts(e)?),
            None => {}
        }
        let condition = match fl.condition() {
            Some(t) => self.expr(t)?,
            None => CastExpr::BoolLiteral {
                value: true,
                meta: m.clone(),
            },
        };
        let mut update: Vec<CastStmt> = per_iteration
            .into_iter()
            .map(|name| CastStmt::ExprStmt {
                expr: CastExpr::Call {
                    function: "__crush_rebind__".to_string(),
                    args: vec![CastExpr::Var {
                        name,
                        meta: m.clone(),
                    }],
                    meta: m.clone(),
                },
                meta: m.clone(),
            })
            .collect();
        if let Some(u) = fl.final_expr() {
            update.extend(self.expr_to_stmts(u)?);
        }
        let mut body = rewrite_continue(self.body(fl.body())?, &update);
        body.extend(update);
        out.push(CastStmt::While {
            condition: Box::new(condition),
            body,
            meta: m.clone(),
        });
        Ok(vec![group(out, m)])
    }

    /// A `for-in`/`for-of` head and body: the loop variable, the body, and
    /// whether a scope was pushed (the caller pops it). `let`/`const` heads
    /// bind a fresh variable each iteration (CRUSH-251 step 4b): iterate a
    /// hidden variable and start the body with a block-binding `x = hidden`.
    fn for_head(
        &mut self,
        head: &IterableLoopInitializer,
        body: &Statement,
        span: Option<Span>,
    ) -> anyhow::Result<(String, Vec<CastStmt>, bool)> {
        match head {
            IterableLoopInitializer::Identifier(id) => {
                let name = self.resolve(id.sym());
                Ok((name, self.body(body)?, false))
            }
            IterableLoopInitializer::Var(v) => {
                let Binding::Identifier(id) = v.binding() else {
                    return Err(self.unsupported("a destructuring loop variable", span));
                };
                if v.init().is_some() {
                    return Err(self.unsupported("an initializer in a `for-in` head", span));
                }
                let name = self.resolve(id.sym());
                Ok((name, self.body(body)?, false))
            }
            IterableLoopInitializer::Let(b) | IterableLoopInitializer::Const(b) => {
                let Binding::Identifier(id) = b else {
                    return Err(self.unsupported("a destructuring loop variable", span));
                };
                self.scopes.push_block();
                let fresh = self.fresh_name(id.sym());
                self.scopes.declare_block(id.sym(), fresh.clone());
                let hidden = self.hidden(&format!("__iter_{fresh}_"));
                let mut lowered = match self.body(body) {
                    Ok(b) => b,
                    Err(e) => {
                        self.scopes.pop_block();
                        return Err(e);
                    }
                };
                let var = CastExpr::Var {
                    name: hidden.clone(),
                    meta: self.meta(span),
                };
                lowered.insert(0, self.var_decl_stmt(fresh, var, true, span));
                Ok((hidden, lowered, true))
            }
            IterableLoopInitializer::Access(_) => Err(self.unsupported(
                "assigning a loop variable to a property (`for (o.x of …)`)",
                span,
            )),
            IterableLoopInitializer::Pattern(_) => {
                Err(self.unsupported("a destructuring loop variable", span))
            }
        }
    }

    /// `switch`: pick the first matching case (or `default` if none matches),
    /// then run every case body from there down until a `break` (JS
    /// fall-through). Lowered as
    ///   let val = <disc>; let start = <index>; while (true) { if (start <= 0) {..} ...; break; }
    /// so `break` exits the switch. All cases share one block scope.
    fn switch(
        &mut self,
        sw: &boa_ast::statement::Switch,
        m: Meta,
    ) -> anyhow::Result<Vec<CastStmt>> {
        let val = self.hidden("__sw_val_");
        let start = self.hidden("__sw_start_");
        let discriminant = self.expr(sw.val())?;
        let mut out = vec![CastStmt::VarDecl {
            name: val.clone(),
            value: discriminant,
            type_hint: CastType::Any,
            meta: m.clone(),
        }];
        let cases = sw.cases();
        let int = |v: usize| CastExpr::IntLiteral {
            value: v as i64,
            meta: m.clone(),
        };
        let var = |n: &str| CastExpr::Var {
            name: n.to_string(),
            meta: m.clone(),
        };
        let assign_start = |v: usize| CastStmt::ExprStmt {
            expr: CastExpr::Call {
                function: "__crush_assign__".to_string(),
                args: vec![var(&start), int(v)],
                meta: m.clone(),
            },
            meta: m.clone(),
        };
        let default_idx = cases.iter().position(|c| c.is_default());
        let fallback = vec![assign_start(default_idx.unwrap_or(cases.len()))];
        let mut chain: Option<Vec<CastStmt>> = None;
        for (i, case) in cases.iter().enumerate().rev() {
            let Some(test) = case.condition() else {
                continue;
            };
            let test = self.expr(test)?;
            chain = Some(vec![CastStmt::If {
                condition: CastExpr::BinaryOp {
                    operator: "===".to_string(),
                    left: Box::new(var(&val)),
                    right: Box::new(test),
                    meta: m.clone(),
                },
                then_body: vec![assign_start(i)],
                else_body: Some(chain.take().unwrap_or_else(|| fallback.clone())),
                meta: m.clone(),
            }]);
        }
        out.push(CastStmt::VarDecl {
            name: start.clone(),
            value: int(cases.len()),
            type_hint: CastType::Any,
            meta: m.clone(),
        });
        out.extend(chain.unwrap_or(fallback));

        self.scopes.push_block();
        let r = (|| {
            let mut syms = Vec::new();
            for c in cases {
                lexical_syms(c.body().statements(), &mut syms);
            }
            self.declare_block_syms(syms);
            let mut body = Vec::new();
            for c in cases {
                body.extend(self.hoisted_functions(c.body().statements(), true)?);
            }
            for (i, case) in cases.iter().enumerate() {
                let stmts = self.items(case.body().statements())?;
                if contains_loop_continue(&stmts) {
                    return Err(self.unsupported("`continue` inside a `switch`", None));
                }
                body.push(CastStmt::If {
                    condition: CastExpr::BinaryOp {
                        operator: "<=".to_string(),
                        left: Box::new(var(&start)),
                        right: Box::new(int(i)),
                        meta: m.clone(),
                    },
                    then_body: stmts,
                    else_body: None,
                    meta: m.clone(),
                });
            }
            body.push(CastStmt::Break { meta: m.clone() });
            Ok(body)
        })();
        self.scopes.pop_block();
        out.push(CastStmt::While {
            condition: Box::new(CastExpr::BoolLiteral {
                value: true,
                meta: m.clone(),
            }),
            body: r?,
            meta: m.clone(),
        });
        Ok(vec![group(out, m)])
    }

    /// An expression in statement position; `a, b` becomes two statements.
    pub(super) fn expr_to_stmts(&mut self, e: &Expression) -> anyhow::Result<Vec<CastStmt>> {
        let mut parts = Vec::new();
        comma_parts(e, &mut parts);
        let mut out = Vec::new();
        for p in parts {
            let meta = self.meta(Some(p.span()));
            out.push(CastStmt::ExprStmt {
                expr: self.expr(p)?,
                meta,
            });
        }
        Ok(out)
    }

    /// The crush local `sym` names (a global keeps its spelling).
    pub(super) fn resolve(&self, sym: Sym) -> String {
        match self.scopes.resolve(sym) {
            Some(n) => n.to_string(),
            None => self.name(sym),
        }
    }
}

/// Every identifier a function's parameters and body mention (bindings and
/// uses; property names aren't identifiers).
fn idents_of(f: &FunctionDeclaration) -> HashSet<Sym> {
    struct Idents(HashSet<Sym>);
    impl<'ast> Visitor<'ast> for Idents {
        type BreakTy = Infallible;
        fn visit_identifier(&mut self, node: &'ast Identifier) -> ControlFlow<Infallible> {
            self.0.insert(node.sym());
            ControlFlow::Continue(())
        }
    }
    let mut v = Idents(HashSet::new());
    let _ = f.parameters().visit_with(&mut v);
    let _ = f.body().visit_with(&mut v);
    v.0
}

/// The parts of a comma expression, in order (`a, b, c`).
pub(super) fn comma_parts<'e>(e: &'e Expression, out: &mut Vec<&'e Expression>) {
    match unparen(e) {
        Expression::Binary(b)
            if b.op() == boa_ast::expression::operator::binary::BinaryOp::Comma =>
        {
            comma_parts(b.lhs(), out);
            comma_parts(b.rhs(), out);
        }
        _ => out.push(e),
    }
}

pub(super) fn unparen(e: &Expression) -> &Expression {
    match e {
        Expression::Parenthesized(p) => unparen(p.expression()),
        other => other,
    }
}
