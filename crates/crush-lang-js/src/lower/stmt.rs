//! Statements, declarations and function bodies.

use boa_ast::declaration::{
    Binding, Declaration, ExportDeclaration, ImportKind, LexicalDeclaration, VarDeclaration,
    Variable,
};
use std::collections::HashSet;
use std::convert::Infallible;
use std::ops::ControlFlow;

use boa_ast::expression::access::{PropertyAccess, PropertyAccessField};
use boa_ast::expression::{Expression, Identifier};
use boa_ast::pattern::{ArrayPatternElement, ObjectPatternElement, Pattern};
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

use super::builtins;
use super::scope::{self, Sig, lexical_syms, param_syms, sig_of, var_syms};
use super::{Lower, Meta, group};
use crate::lower_common::{contains_loop_continue, for_in_guarded, js_cap};

/// Where `function` declarations are hoisted.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Hoist {
    /// The script's top level (`main`).
    Script,
    /// A function's body.
    Function,
    /// A block (or a `switch`): a fresh binding each time it runs.
    Block,
}

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
        // Classes register (names + signatures) before any function
        // body lowers: the bodies only run after the whole module, so
        // they may construct classes declared after them (CRUSH-281).
        for item in &list {
            self.preregister_kinds(std::slice::from_ref(*item));
        }
        for item in items {
            if let TopItem::Module(ModuleItem::ExportDeclaration(e)) = item {
                self.preregister_export(e);
            }
        }

        let mut body = Vec::new();
        for item in &list {
            body.extend(self.uninit_vars(std::slice::from_ref(*item)));
        }
        let mut owned: Vec<StatementListItem> = list.iter().map(|i| (*i).clone()).collect();
        // An exported `function` declaration hoists exactly like a bare one
        // (CRUSH-340): without this it never reached the hoisting pass, and
        // `export()` refused it, so no ES module's functions lowered at all.
        for item in items {
            if let TopItem::Module(ModuleItem::ExportDeclaration(e)) = item
                && let ExportDeclaration::Declaration(Declaration::FunctionDeclaration(f)) = &**e
            {
                owned.push(StatementListItem::Declaration(Box::new(
                    Declaration::FunctionDeclaration(f.clone()),
                )));
            }
        }
        body.extend(self.hoisted_functions(&owned, Hoist::Script)?);
        for item in items {
            match item {
                TopItem::Module(ModuleItem::ImportDeclaration(import)) => {
                    body.push(self.import(import))
                }
                TopItem::Module(ModuleItem::ExportDeclaration(e)) => body.extend(self.export(e)?),
                _ => {
                    let s = item.stmt().expect("a statement");
                    body.extend(self.item(s)?);
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
            ExportDeclaration::Declaration(Declaration::ClassDeclaration(c)) => {
                let mut out = self.class_decl(c)?;
                let name = self.resolve(c.name().sym());
                out.push(export(self, name.clone(), var(self, name)));
                Ok(out)
            }
            ExportDeclaration::ReExport { .. } => {
                Err(self.unsupported("a re-export (`export … from …`)", None))
            }
            // Hoisted by `top_level` like a bare declaration (CRUSH-340);
            // only the export itself is emitted here.
            ExportDeclaration::Declaration(Declaration::FunctionDeclaration(f)) => {
                let name = self.resolve(f.name().sym());
                Ok(vec![export(self, name.clone(), var(self, name))])
            }
            ExportDeclaration::Declaration(_)
            | ExportDeclaration::DefaultGeneratorDeclaration(_)
            | ExportDeclaration::DefaultAsyncFunctionDeclaration(_)
            | ExportDeclaration::DefaultAsyncGeneratorDeclaration(_)
            | ExportDeclaration::DefaultClassDeclaration(_) => Err(self.unsupported(
                "exporting a generator or async declaration, or a default-exported declaration",
                None,
            )),
        }
    }

    // ── Functions ───────────────────────────────────────────────────────────

    /// A function's parameters and body, in a new crush function scope. The
    /// caller decides whether it sees the enclosing locals (a lambda) or not
    /// (a top-level function: [`super::scope::Scopes::isolate`]).
    ///
    /// `declared`: a `function` declaration, whose calls are adapted to its
    /// parameters ([`super::scope::Param`]), so it may have default and rest
    /// parameters. A function expression or arrow function may not: it can be
    /// called from anywhere (CRUSH-zorro-18). Class members take the same
    /// adapted mode through `method_function`/`static_function`
    /// (CRUSH-280).
    pub(super) fn function(
        &mut self,
        params: &FormalParameterList,
        body: &FunctionBody,
        span: Option<Span>,
        declared: bool,
    ) -> anyhow::Result<LoweredFn> {
        self.scopes.enter_fn();
        // A nested function's `this` is its own (an arrow's would be the
        // enclosing method's — not lowered yet), never the method's
        // receiver (CRUSH-277 D2). `super(...)` and the instance
        // bindings of the enclosing function don't cross either (S3).
        let saved = std::mem::replace(&mut self.this_bound, false);
        let saved_super = self.super_ctor.take();
        let saved_instances = std::mem::take(&mut self.instance_kinds);
        let saved_hosts = std::mem::take(&mut self.host_kinds);
        let saved_home = self.home_class.take();
        let saved_static = std::mem::replace(&mut self.in_static, false);
        let saved_finally = std::mem::take(&mut self.finally_stack);
        let saved_jumps = std::mem::take(&mut self.jump_targets);
        let saved_signals = std::mem::take(&mut self.completion_signals);
        let saved_try_depth = std::mem::replace(&mut self.try_depth, 0);
        let saved_loop_depth = std::mem::replace(&mut self.loop_depth, 0);
        let saved_switch_floors = std::mem::take(&mut self.switch_loop_floors);
        let r = self.function_in_scope(params, body, span, declared);
        self.finally_stack = saved_finally;
        self.jump_targets = saved_jumps;
        self.completion_signals = saved_signals;
        self.try_depth = saved_try_depth;
        self.loop_depth = saved_loop_depth;
        self.switch_loop_floors = saved_switch_floors;
        self.this_bound = saved;
        self.super_ctor = saved_super;
        self.instance_kinds = saved_instances;
        self.host_kinds = saved_hosts;
        self.home_class = saved_home;
        self.in_static = saved_static;
        self.scopes.leave_fn();
        r
    }

    /// A class method, constructor or accessor body (CRUSH-277 D2): like a
    /// function expression, but `this` is bound — the receiver the VM
    /// passes as a method's leading argument — so it becomes the first
    /// CAST parameter, and `this` expressions read it. Default and rest
    /// parameters are allowed (CRUSH-280): the walker knows every call
    /// path to a member and adapts the arguments at each (the
    /// CRUSH-zorro-18 hidden-flag ABI), so `function_in_scope` runs in
    /// its adapted mode.
    pub(super) fn method_function(
        &mut self,
        params: &FormalParameterList,
        body: &FunctionBody,
        span: Option<Span>,
    ) -> anyhow::Result<LoweredFn> {
        self.scopes.enter_fn();
        let saved = std::mem::replace(&mut self.this_bound, true);
        let saved_instances = std::mem::take(&mut self.instance_kinds);
        let saved_hosts = std::mem::take(&mut self.host_kinds);
        let saved_finally = std::mem::take(&mut self.finally_stack);
        let saved_jumps = std::mem::take(&mut self.jump_targets);
        let saved_signals = std::mem::take(&mut self.completion_signals);
        let saved_try_depth = std::mem::replace(&mut self.try_depth, 0);
        let saved_loop_depth = std::mem::replace(&mut self.loop_depth, 0);
        let saved_switch_floors = std::mem::take(&mut self.switch_loop_floors);
        let r = self.function_in_scope(params, body, span, true);
        self.finally_stack = saved_finally;
        self.jump_targets = saved_jumps;
        self.completion_signals = saved_signals;
        self.try_depth = saved_try_depth;
        self.loop_depth = saved_loop_depth;
        self.switch_loop_floors = saved_switch_floors;
        self.this_bound = saved;
        self.instance_kinds = saved_instances;
        self.host_kinds = saved_hosts;
        self.scopes.leave_fn();
        let (mut lowered_params, body) = r?;
        lowered_params.insert(0, ("this".to_string(), CastType::Any));
        Ok((lowered_params, body))
    }

    /// A static method body (CRUSH-277 D8): the VM calls a static with no
    /// receiver, so its parameters are the plain CAST parameters;
    /// `this` in JS is the class itself, bound here as a local to the
    /// class's kind value. (An inherited static's `this` is therefore the
    /// defining class, not the subclass it was called through.)
    pub(super) fn static_function(
        &mut self,
        kind: &str,
        params: &FormalParameterList,
        body: &FunctionBody,
        span: Option<Span>,
    ) -> anyhow::Result<LoweredFn> {
        self.scopes.enter_fn();
        let saved = std::mem::replace(&mut self.this_bound, true);
        let saved_super = self.super_ctor.take();
        let saved_instances = std::mem::take(&mut self.instance_kinds);
        let saved_hosts = std::mem::take(&mut self.host_kinds);
        let saved_static = std::mem::replace(&mut self.in_static, true);
        let saved_finally = std::mem::take(&mut self.finally_stack);
        let saved_jumps = std::mem::take(&mut self.jump_targets);
        let saved_signals = std::mem::take(&mut self.completion_signals);
        let saved_try_depth = std::mem::replace(&mut self.try_depth, 0);
        let saved_loop_depth = std::mem::replace(&mut self.loop_depth, 0);
        let saved_switch_floors = std::mem::take(&mut self.switch_loop_floors);
        let r = self.function_in_scope(params, body, span, true);
        self.finally_stack = saved_finally;
        self.jump_targets = saved_jumps;
        self.completion_signals = saved_signals;
        self.try_depth = saved_try_depth;
        self.loop_depth = saved_loop_depth;
        self.switch_loop_floors = saved_switch_floors;
        self.this_bound = saved;
        self.super_ctor = saved_super;
        self.instance_kinds = saved_instances;
        self.host_kinds = saved_hosts;
        self.in_static = saved_static;
        self.scopes.leave_fn();
        let (lowered_params, mut lowered_body) = r?;
        let this_value = CastExpr::Var {
            name: kind.to_string(),
            meta: self.meta(span),
        };
        lowered_body.insert(0, self.var_decl_stmt("this".to_string(), this_value, false, span));
        Ok((lowered_params, lowered_body))
    }

    fn function_in_scope(
        &mut self,
        params: &FormalParameterList,
        body: &FunctionBody,
        span: Option<Span>,
        declared: bool,
    ) -> anyhow::Result<LoweredFn> {
        // Assignments inside a nested function poison the bindings
        // they touch (CRUSH-286 re-review): `fn_depth` is how the
        // assignment lowering knows it is inside one.
        self.fn_depth += 1;
        let r = self.function_in_scope_body(params, body, span, declared);
        self.fn_depth -= 1;
        r
    }

    fn function_in_scope_body(
        &mut self,
        params: &FormalParameterList,
        body: &FunctionBody,
        span: Option<Span>,
        declared: bool,
    ) -> anyhow::Result<LoweredFn> {
        let mut lowered_params = Vec::new();
        // Prologue steps in parameter order: a default runs when its
        // hidden flag says no argument was given, and a destructuring
        // parameter then binds its names from the (possibly defaulted)
        // value (CRUSH-284).
        enum Prologue<'x> {
            Default(String, &'x Expression),
            Destructure(String, &'x Pattern),
        }
        let mut steps: Vec<Prologue<'_>> = Vec::new();
        for p in params.as_ref() {
            if !declared && p.is_rest_param() {
                return Err(self.unsupported(
                    "a rest parameter (`...args`) outside a `function` declaration",
                    span,
                ));
            }
            if let Some(init) = p.init()
                && !declared
            {
                return Err(self.unsupported(
                    "a default parameter value (`x = …`) outside a `function` declaration",
                    Some(init.span()),
                ));
            }
            match p.variable().binding() {
                Binding::Identifier(id) => {
                    let name = self.name(id.sym());
                    self.scopes.declare_root(id.sym(), name.clone());
                    lowered_params.push((name.clone(), CastType::Any));
                    if let Some(init) = p.init() {
                        steps.push(Prologue::Default(name, init));
                    }
                }
                Binding::Pattern(pat) => {
                    // A destructuring parameter is one visible
                    // parameter holding the value; the prologue binds
                    // its names from it (CRUSH-284).
                    let value = self.hidden("__destr");
                    let mut syms = Vec::new();
                    scope::binding_syms(p.variable().binding(), &mut syms);
                    for sym in syms {
                        let name = self.name(sym);
                        self.scopes.declare_root(sym, name);
                    }
                    lowered_params.push((value.clone(), CastType::Any));
                    if let Some(init) = p.init() {
                        steps.push(Prologue::Default(value.clone(), init));
                    }
                    steps.push(Prologue::Destructure(value, pat));
                }
            }
        }
        // Defaults run first, in parameter order, each when its argument
        // wasn't given (the hidden flag after the parameters). They're
        // lowered before the body's names are declared, so they see the
        // parameters and the enclosing scope, as in JS.
        let mut prologue = Vec::new();
        for step in steps {
            let (name, init) = match step {
                Prologue::Default(name, init) => (name, init),
                Prologue::Destructure(value, pat) => {
                    let source = CastExpr::Var {
                        name: value,
                        meta: self.meta(span),
                    };
                    self.destructure(source, pat, None, span, &mut prologue)?;
                    continue;
                }
            };
            let flag = self.hidden(&format!("__has_{name}_"));
            let m = self.meta(Some(init.span()));
            let value = self.expr(init)?;
            prologue.push(CastStmt::If {
                condition: CastExpr::UnaryOp {
                    operator: "!".to_string(),
                    operand: Box::new(CastExpr::Var {
                        name: flag.clone(),
                        meta: m.clone(),
                    }),
                    meta: m.clone(),
                },
                then_body: vec![CastStmt::ExprStmt {
                    expr: CastExpr::Call {
                        function: "__crush_assign__".to_string(),
                        args: vec![
                            CastExpr::Var {
                                name,
                                meta: m.clone(),
                            },
                            value,
                        ],
                        meta: m.clone(),
                    },
                    meta: m.clone(),
                }],
                else_body: None,
                meta: m,
            });
            lowered_params.push((flag, CastType::Any));
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
        // Classes in this body register before its functions lower,
        // as at the top level (CRUSH-281). The registration is this
        // list's alone: it is restored away when the body is done, so
        // a class local to one function is invisible to its siblings.
        let saved_kinds = self.visible_kinds.clone();
        let saved_bindings = self.class_bindings.clone();
        self.preregister_kinds(items);
        let mut out = prologue;
        out.extend(self.uninit_vars(items));
        out.extend(self.hoisted_functions(items, Hoist::Function)?);
        out.extend(self.items(items)?);
        self.visible_kinds = saved_kinds;
        self.class_bindings = saved_bindings;
        Ok((lowered_params, out))
    }

    /// Bind a destructuring pattern's names from its value
    /// (CRUSH-284): each property or element is read once, a
    /// per-element default replaces an *absent* value — JS's
    /// `undefined`; a value present as null keeps null, so absence is
    /// tested (a has-key check, the length guard), never `== null` —
    /// and nested patterns recurse. Object/array rest elements and
    /// assignment targets stay loud.
    ///
    /// `decl` says how a leaf name is bound: `None` for a parameter
    /// (a plain local), `Some(block)` for a declaration (CRUSH-287),
    /// which assigns instead when the binding was created on entry
    /// to its block, exactly like an identifier declaration.
    fn destructure(
        &mut self,
        source: CastExpr,
        pattern: &Pattern,
        decl: Option<bool>,
        span: Option<Span>,
        out: &mut Vec<CastStmt>,
    ) -> anyhow::Result<()> {
        match pattern {
            Pattern::Object(o) => {
                for el in o.bindings() {
                    match el {
                        ObjectPatternElement::SingleName {
                            name,
                            ident,
                            default_init,
                        } => {
                            let field = self.property_name(name, span)?;
                            let value = self.object_element_value(
                                &source,
                                &field,
                                default_init.as_ref(),
                                span,
                                out,
                            )?;
                            self.bind_leaf(ident.sym(), value, decl, span, out);
                        }
                        ObjectPatternElement::Pattern {
                            name,
                            pattern,
                            default_init,
                        } => {
                            let field = self.property_name(name, span)?;
                            let value = self.object_element_value(
                                &source,
                                &field,
                                default_init.as_ref(),
                                span,
                                out,
                            )?;
                            self.destructure(value, pattern, decl, span, out)?;
                        }
                        ObjectPatternElement::RestProperty { .. } => {
                            return Err(self.unsupported(
                                "an object rest property (`{...rest}`) in a destructuring parameter",
                                span,
                            ));
                        }
                        ObjectPatternElement::AssignmentPropertyAccess { .. }
                        | ObjectPatternElement::AssignmentRestPropertyAccess { .. } => {
                            return Err(self.unsupported(
                                "an assignment target in a destructuring parameter",
                                span,
                            ));
                        }
                    }
                }
            }
            Pattern::Array(a) => {
                for (i, el) in a.bindings().iter().enumerate() {
                    match el {
                        ArrayPatternElement::Elision => {}
                        ArrayPatternElement::SingleName {
                            ident,
                            default_init,
                        } => {
                            let value = self.array_element_value(
                                &source,
                                i,
                                default_init.as_ref(),
                                span,
                                out,
                            )?;
                            self.bind_leaf(ident.sym(), value, decl, span, out);
                        }
                        ArrayPatternElement::Pattern {
                            pattern,
                            default_init,
                        } => {
                            let value = self.array_element_value(
                                &source,
                                i,
                                default_init.as_ref(),
                                span,
                                out,
                            )?;
                            self.destructure(value, pattern, decl, span, out)?;
                        }
                        ArrayPatternElement::SingleNameRest { .. }
                        | ArrayPatternElement::PatternRest { .. } => {
                            return Err(self.unsupported(
                                "an array rest element (`[...rest]`) in a destructuring parameter",
                                span,
                            ));
                        }
                        ArrayPatternElement::PropertyAccess { .. }
                        | ArrayPatternElement::PropertyAccessRest { .. } => {
                            return Err(self.unsupported(
                                "an assignment target in a destructuring parameter",
                                span,
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Bind one destructured name to its value: a declaration of an
    /// entry-created (predeclared) block binding assigns it, anything
    /// else declares it — mirroring an identifier declaration
    /// (CRUSH-287).
    fn bind_leaf(
        &mut self,
        sym: Sym,
        value: CastExpr,
        decl: Option<bool>,
        span: Option<Span>,
        out: &mut Vec<CastStmt>,
    ) {
        let name = self.resolve(sym);
        match decl {
            None => out.push(self.var_decl_stmt(name, value, false, span)),
            Some(block) if block && self.scopes.is_predeclared(sym) => {
                let m = self.meta(span);
                out.push(CastStmt::ExprStmt {
                    expr: CastExpr::Call {
                        function: "__crush_assign__".to_string(),
                        args: vec![
                            CastExpr::Var {
                                name,
                                meta: m.clone(),
                            },
                            value,
                        ],
                        meta: m.clone(),
                    },
                    meta: m,
                });
            }
            Some(block) => out.push(self.var_decl_stmt(name, value, block, span)),
        }
    }

    /// An object property for destructuring (CRUSH-284): read once
    /// into a hidden temporary; the per-property default applies only
    /// when the property is *absent* (`__crush_has__`, the CASM
    /// `has_member`) — a property present with value null keeps its
    /// null, exactly as JS's `undefined` test behaves.
    fn object_element_value(
        &mut self,
        source: &CastExpr,
        field: &str,
        default_init: Option<&Expression>,
        span: Option<Span>,
        out: &mut Vec<CastStmt>,
    ) -> anyhow::Result<CastExpr> {
        let m = self.meta(span);
        let temp = self.hidden("__dval");
        out.push(self.var_decl_stmt(
            temp.clone(),
            CastExpr::GetField {
                target: Box::new(source.clone()),
                field: field.to_string(),
                meta: m.clone(),
            },
            false,
            span,
        ));
        let temp_var = CastExpr::Var {
            name: temp.clone(),
            meta: m.clone(),
        };
        if let Some(init) = default_init {
            let value = self.expr(init)?;
            out.push(CastStmt::If {
                condition: CastExpr::UnaryOp {
                    operator: "!".to_string(),
                    operand: Box::new(CastExpr::Call {
                        function: "__crush_has__".to_string(),
                        args: vec![
                            source.clone(),
                            CastExpr::StringLiteral {
                                value: field.to_string(),
                                meta: m.clone(),
                            },
                        ],
                        meta: m.clone(),
                    }),
                    meta: m.clone(),
                },
                then_body: vec![CastStmt::ExprStmt {
                    expr: CastExpr::Call {
                        function: "__crush_assign__".to_string(),
                        args: vec![temp_var.clone(), value],
                        meta: m.clone(),
                    },
                    meta: m.clone(),
                }],
                else_body: None,
                meta: m,
            });
        }
        Ok(temp_var)
    }

    /// An array element for destructuring (CRUSH-284): the read is
    /// guarded by the length — past the end the element is absent
    /// (JS's `undefined`, never a bounds error) and the per-element
    /// default applies in the guard's `else`; an element present as
    /// null keeps its null.
    fn array_element_value(
        &mut self,
        source: &CastExpr,
        i: usize,
        default_init: Option<&Expression>,
        span: Option<Span>,
        out: &mut Vec<CastStmt>,
    ) -> anyhow::Result<CastExpr> {
        let m = self.meta(span);
        let temp = self.hidden("__dval");
        out.push(self.var_decl_stmt(
            temp.clone(),
            CastExpr::NullLiteral { meta: m.clone() },
            false,
            span,
        ));
        let temp_var = CastExpr::Var {
            name: temp.clone(),
            meta: m.clone(),
        };
        let else_body = match default_init {
            Some(init) => {
                let value = self.expr(init)?;
                Some(vec![CastStmt::ExprStmt {
                    expr: CastExpr::Call {
                        function: "__crush_assign__".to_string(),
                        args: vec![temp_var.clone(), value],
                        meta: m.clone(),
                    },
                    meta: m.clone(),
                }])
            }
            None => None,
        };
        out.push(CastStmt::If {
            condition: CastExpr::BinaryOp {
                operator: "<".to_string(),
                left: Box::new(CastExpr::IntLiteral {
                    value: i as i64,
                    meta: m.clone(),
                }),
                right: Box::new(crate::lower_common::length_of(
                    source.clone(),
                    m.clone(),
                )),
                meta: m.clone(),
            },
            then_body: vec![CastStmt::ExprStmt {
                expr: CastExpr::Call {
                    function: "__crush_assign__".to_string(),
                    args: vec![
                        temp_var,
                        CastExpr::Index {
                            target: Box::new(source.clone()),
                            index: Box::new(CastExpr::IntLiteral {
                                value: i as i64,
                                meta: m.clone(),
                            }),
                            meta: m.clone(),
                        },
                    ],
                    meta: m.clone(),
                },
                meta: m.clone(),
            }],
            else_body,
            meta: m,
        });
        Ok(CastExpr::Var {
            name: temp,
            meta: self.meta(span),
        })
    }

    /// `function f() {}` in a scope, before the other statements of the
    /// scope run (hoisting):
    /// - one that uses no binding of an enclosing function (at the script
    ///   level: no script binding) is **lifted** to a crush function of its
    ///   own (calls are direct, so AOT can compile them). A nested one gets a
    ///   fresh name; a script-level one keeps its own;
    /// - one that does is a **closure** bound to `f`, capturing those bindings
    ///   by reference (CRUSH-251 cells). The scope's `let`/`const` bindings are
    ///   then created first ([`Lower::predeclare`]), so a closure made here
    ///   sees the binding a later `let` initialises.
    fn hoisted_functions(
        &mut self,
        items: &[StatementListItem],
        at: Hoist,
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
        // Every declaration's parameters, for adapting its calls.
        let sigs: Vec<Sig> = decls.iter().map(|f| sig_of(f.parameters())).collect();
        for (f, sig) in decls.iter().zip(&sigs) {
            self.scopes.set_sig(f.name().sym(), sig.clone());
        }
        // Rebind the lifted names before lowering anything that uses them.
        let mut lifted = Vec::new();
        for (i, f) in decls.iter().enumerate() {
            if lift[i] {
                let sym = f.name().sym();
                let global = if at == Hoist::Script {
                    // A script-level function keeps its name: a global, not a capture.
                    self.toplevel_fns.insert(sym);
                    self.name(sym)
                } else {
                    self.fresh_name(sym)
                };
                if global == "main" {
                    // `main` is the crush function the script body becomes.
                    return Err(self
                        .unsupported("a top-level function named `main`", Some(f.name().span())));
                }
                self.scopes.declare_block(sym, global.clone());
                if at == Hoist::Script {
                    self.toplevel_sigs.insert(sym, sigs[i].clone());
                }
                lifted.push((sym, global, sigs[i].clone()));
            }
        }
        let mut out = Vec::new();
        if lift.iter().any(|l| !l) {
            out.extend(self.predeclare(items, at));
        }
        for (i, f) in decls.iter().enumerate() {
            let span = Some(f.name().span());
            if f.parameters().as_ref().iter().any(|p| p.is_rest_param()) {
                let sig = &sigs[i];
                if let Some(global) = lifted.iter().find(|(sym, _, _)| *sym == f.name().sym()).map(|(_, name, _)| name.clone()) {
                    self.fn_sigs.insert(global, sig.clone());
                }
            }
            if lift[i] {
                let saved = self.scopes.isolate();
                self.scopes.enter_fn();
                for (sym, global, sig) in &lifted {
                    self.scopes.declare_root(*sym, global.clone());
                    self.scopes.set_sig(*sym, sig.clone());
                }
                let lowered = self.function(f.parameters(), f.body(), span, true);
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
                let (params, body) = self.function(f.parameters(), f.body(), span, true)?;
                let value = CastExpr::Lambda {
                    params,
                    body,
                    meta: self.meta(span),
                };
                out.push(self.var_decl_stmt(name, value, at == Hoist::Block, span));
            }
        }
        Ok(out)
    }

    /// Create a scope's `let`/`const` bindings on entry (`x = null`; a fresh
    /// one per entry in a block), so closures hoisted ahead of the
    /// declarations capture the same binding. The declarations then assign.
    fn predeclare(&mut self, items: &[StatementListItem], at: Hoist) -> Vec<CastStmt> {
        let mut syms = Vec::new();
        for item in items {
            if let StatementListItem::Declaration(d) = item
                && let Declaration::Lexical(lex) = d.as_ref()
            {
                scope::lexical_decl_syms(lex, &mut syms);
            }
        }
        syms.into_iter()
            .map(|sym| {
                self.scopes.predeclare(sym);
                let name = self.resolve(sym);
                let value = CastExpr::NullLiteral {
                    meta: self.meta(None),
                };
                self.var_decl_stmt(name, value, at == Hoist::Block, None)
            })
            .collect()
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
        // A block is a statement list of its own (CRUSH-281): its
        // classes pre-register for its nested functions, and the
        // registration is restored away at the end of the block.
        let saved_kinds = self.visible_kinds.clone();
        let saved_bindings = self.class_bindings.clone();
        self.preregister_kinds(items);
        let r = (|| {
            let mut out = self.hoisted_functions(items, Hoist::Block)?;
            out.extend(self.items(items)?);
            Ok(out)
        })();
        self.visible_kinds = saved_kinds;
        self.class_bindings = saved_bindings;
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
            // The class itself rides in `Program::cosm` and its methods in
            // `Program::functions` (CRUSH-277 S1); materialising the class
            // name as a value is S3, and nothing runs until the VM slice
            // (S2) lifts the prototype-chain gate.
            Declaration::ClassDeclaration(c) => self.class_decl(c),
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
        if let Binding::Pattern(pat) = v.binding() {
            return self.destructuring_decl(v, pat, block);
        }
        let Binding::Identifier(id) = v.binding() else {
            unreachable!("bindings are identifiers or patterns");
        };
        let name = self.resolve(id.sym());
        // `const c = new Counter()`: the binding holds an instance of a
        // known kind, so `c.m()` lowers to a member call (CRUSH-277 D4).
        if self.branch_depth > 0 {
            self.flow_assigned.insert(name.clone());
        }
        if let Some(kind) = v.init().and_then(|init| self.new_kind(init)) {
            self.instance_kinds.insert(name.clone(), kind);
            self.host_kinds.remove(&name);
        } else if let Some(kind) = v.init().and_then(|init| self.host_kind_of(init)) {
            // `const m = new Map()`: a proven host collection, so
            // `m.set(...)` lowers to the helpers (CRUSH-288).
            self.host_kinds.insert(name.clone(), kind);
            self.instance_kinds.remove(&name);
        }
        let span = v.init().map(Spanned::span);
        let mut value = match v.init() {
            Some(init) => self.expr(init)?,
            // `var x;` is hoisted (see `uninit_vars`): running it again, in a
            // loop or after an assignment, must not reset `x`.
            None if !block => return Ok(vec![]),
            None => CastExpr::NullLiteral {
                meta: self.meta(span),
            },
        };
        // `const C = class { … }`: the initializer is the class itself,
        // which the compiler reads by kind name (CRUSH-277 D8) — but it
        // counts the binding declared while compiling the initializer, so
        // a direct read would load the uninitialised binding. Go through
        // a hidden temporary the binding can't shadow.
        let mut prefix = Vec::new();
        if v.init().is_some_and(|init| matches!(unparen(init), Expression::ClassExpression(_)))
            && matches!(&value, CastExpr::Var { name: n, .. } if n == &name)
        {
            let temp = self.hidden(&format!("{name}__class"));
            prefix.push(self.var_decl_stmt(temp.clone(), value, block, span));
            value = CastExpr::Var {
                name: temp,
                meta: self.meta(span),
            };
        }
        if block && self.scopes.is_predeclared(id.sym()) {
            // Created on entry to the scope (`predeclare`): assign it.
            let m = self.meta(span);
            let target = CastExpr::Var {
                name,
                meta: m.clone(),
            };
            let expr = CastExpr::Call {
                function: "__crush_assign__".to_string(),
                args: vec![target, value],
                meta: m.clone(),
            };
            prefix.push(CastStmt::ExprStmt { expr, meta: m });
            return Ok(prefix);
        }
        prefix.push(self.var_decl_stmt(name, value, block, span));
        Ok(prefix)
    }

    /// A destructuring declaration (CRUSH-287): the initializer is
    /// evaluated once into a hidden temporary, then the pattern binds
    /// its names from it. Without an initializer the names bind null
    /// (JS's `undefined`) — no destructuring happens.
    fn destructuring_decl(
        &mut self,
        v: &Variable,
        pat: &Pattern,
        block: bool,
    ) -> anyhow::Result<Vec<CastStmt>> {
        let span = v.init().map(Spanned::span);
        let mut syms = Vec::new();
        scope::binding_syms(v.binding(), &mut syms);
        // What a branch assigns must be forgotten after it, as for
        // identifier declarations.
        if self.branch_depth > 0 {
            for sym in &syms {
                self.flow_assigned.insert(self.resolve(*sym));
            }
        }
        let mut out = Vec::new();
        let Some(init) = v.init() else {
            for sym in syms {
                let value = CastExpr::NullLiteral {
                    meta: self.meta(span),
                };
                self.bind_leaf(sym, value, Some(block), span, &mut out);
            }
            return Ok(out);
        };
        let value = self.expr(init)?;
        let temp = self.hidden("__dsrc");
        out.push(self.var_decl_stmt(temp.clone(), value, false, span));
        let source = CastExpr::Var {
            name: temp,
            meta: self.meta(span),
        };
        self.destructure(source, pat, Some(block), span, &mut out)?;
        Ok(out)
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

    /// Emit the active finally bodies from the top of the stack down to
    /// `keep_depth` (innermost first). `cleanup(0)` runs every active finally.
    fn cleanup(&self, keep_depth: usize) -> Vec<CastStmt> {
        let keep = keep_depth.min(self.finally_stack.len());
        self.finally_stack[keep..]
            .iter()
            .rev()
            .flat_map(|body| body.iter().cloned())
            .collect()
    }

    fn jump(&mut self, is_continue: bool, meta: Meta) -> anyhow::Result<Vec<CastStmt>> {
        if is_continue
            && self.switch_loop_floors.last().is_some_and(|floor| self.loop_depth <= *floor)
        {
            return Err(self.unsupported("`continue` inside a `switch`", None));
        }
        let target = if is_continue {
            self.jump_targets.iter().rev().find(|target| target.is_loop)
        } else {
            self.jump_targets.last()
        }
        .cloned()
        .ok_or_else(|| self.unsupported("a `break` or `continue` without a target", None))?;
        if is_continue && target.is_switch {
            return Err(self.unsupported("`continue` inside a `switch`", None));
        }
        let action = if is_continue { target.continue_action } else { target.break_action };
        if target.try_depth == self.try_depth && target.finally_depth == self.finally_stack.len() {
            return Ok(action);
        }
        // Unique string token (not an empty object): CAST `===` lowers to
        // Value::eq, and two empty records compare equal structurally.
        let token = self.hidden("__crush_completion_");
        self.completion_signals.push(super::CompletionSignal {
            token: token.clone(),
            target_try_depth: target.try_depth,
            action,
        });
        Ok(vec![CastStmt::Throw {
            value: CastExpr::StringLiteral { value: token, meta: meta.clone() },
            meta,
        }])
    }

    fn completion_condition(&self, name: &str, signals: &[super::CompletionSignal], meta: &Meta) -> Option<CastExpr> {
        signals.iter().map(|signal| CastExpr::BinaryOp {
            operator: "==".to_string(),
            left: Box::new(CastExpr::Var { name: name.to_string(), meta: meta.clone() }),
            right: Box::new(CastExpr::StringLiteral { value: signal.token.clone(), meta: meta.clone() }),
            meta: meta.clone(),
        }).reduce(|left, right| CastExpr::BinaryOp {
            operator: "||".to_string(), left: Box::new(left), right: Box::new(right), meta: meta.clone(),
        })
    }

    fn dispatch_finally(
        &self,
        error_var: &str,
        finalizer: &[CastStmt],
        boundary_try_depth: usize,
        signals: &[super::CompletionSignal],
        meta: &Meta,
    ) -> Vec<CastStmt> {
        let rethrow = CastStmt::Throw {
            value: CastExpr::Var { name: error_var.to_string(), meta: meta.clone() }, meta: meta.clone(),
        };
        let mut handler = finalizer.to_vec();
        handler.push(rethrow.clone());
        for signal in signals.iter().rev() {
            let mut action = finalizer.to_vec();
            if signal.target_try_depth == boundary_try_depth {
                action.extend(signal.action.iter().cloned());
            } else {
                action.push(rethrow.clone());
            }
            handler = vec![CastStmt::If {
                condition: CastExpr::BinaryOp {
                    operator: "==".to_string(),
                    left: Box::new(CastExpr::Var { name: error_var.to_string(), meta: meta.clone() }),
                    right: Box::new(CastExpr::StringLiteral { value: signal.token.clone(), meta: meta.clone() }),
                    meta: meta.clone(),
                },
                then_body: action,
                else_body: Some(handler),
                meta: meta.clone(),
            }];
        }
        handler
    }

    fn loop_target(&mut self, continue_prefix: &[CastStmt]) -> super::JumpTarget {
        let meta = self.meta(None);
        let mut continue_action = continue_prefix.to_vec();
        continue_action.push(CastStmt::Continue { meta: meta.clone() });
        super::JumpTarget {
            finally_depth: self.finally_stack.len(),
            try_depth: self.try_depth,
            is_switch: false,
            is_loop: true,
            break_action: vec![CastStmt::Break { meta: meta.clone() }],
            continue_action,
        }
    }

    fn loop_body_with_prefix(&mut self, body: &Statement, prefix: &[CastStmt]) -> anyhow::Result<Vec<CastStmt>> {
        let target = self.loop_target(prefix);
        self.jump_targets.push(target);
        self.loop_depth += 1;
        let result = self.body(body);
        self.loop_depth -= 1;
        self.jump_targets.pop();
        result
    }

    pub(super) fn stmt(&mut self, s: &Statement) -> anyhow::Result<Vec<CastStmt>> {
        let span = stmt_span(s);
        let m = self.meta(span);
        // What a branch or loop body assigns must not change what code
        // after it believes about a binding's kind (CRUSH-286 review
        // #3): around a control statement the tracked kinds are
        // restored, and every binding assigned inside it is forgotten
        // — the branch may not have run, so only straight-line `new`
        // assignments teach the tracker, exactly the ones whose value
        // provably holds afterwards.
        let control = matches!(
            s,
            Statement::If(_)
                | Statement::WhileLoop(_)
                | Statement::DoWhileLoop(_)
                | Statement::ForLoop(_)
                | Statement::ForInLoop(_)
                | Statement::ForOfLoop(_)
                | Statement::Switch(_)
                | Statement::Try(_)
        );
        let saved_kinds = control.then(|| self.instance_kinds.clone());
        let saved_hosts = control.then(|| self.host_kinds.clone());
        if control {
            self.branch_depth += 1;
        }
        // The loop back-edge (CRUSH-286 re-review): a binding assigned
        // anywhere in a loop may hold that assigned value from the
        // second iteration on, so no call inside the loop may be
        // shaped for the kind it held on entry. Forget those bindings
        // before the loop (condition included) is lowered; the
        // wrapper's restore below still applies after it.
        if matches!(
            s,
            Statement::WhileLoop(_)
                | Statement::DoWhileLoop(_)
                | Statement::ForLoop(_)
                | Statement::ForInLoop(_)
                | Statement::ForOfLoop(_)
        ) {
            let mut syms = Vec::new();
            super::scope::assigned_syms_stmt(s, &mut syms);
            for sym in syms {
                let name = self.resolve(sym);
                self.instance_kinds.remove(&name);
                self.host_kinds.remove(&name);
            }
        }
        let r = self.stmt_dispatch(s, span, m);
        if control {
            self.branch_depth -= 1;
            self.instance_kinds = saved_kinds.expect("control statements save the kinds");
            self.host_kinds = saved_hosts.expect("control statements save the kinds");
            let assigned = std::mem::take(&mut self.flow_assigned);
            for name in &assigned {
                self.instance_kinds.remove(name);
                self.host_kinds.remove(name);
            }
            if self.branch_depth > 0 {
                // An enclosing control statement forgets them too.
                self.flow_assigned = assigned;
            }
        }
        r
    }

    fn stmt_dispatch(
        &mut self,
        s: &Statement,
        span: Option<Span>,
        m: Meta,
    ) -> anyhow::Result<Vec<CastStmt>> {
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
                let body = self.loop_body_with_prefix(w.body(), &[])?;
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
                let mut body = self.loop_body_with_prefix(dw.body(), std::slice::from_ref(&exit_check))?;
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
                let target = self.loop_target(&[]);
                self.jump_targets.push(target);
                self.loop_depth += 1;
                let result = self.for_head(fi.initializer(), fi.body(), span);
                self.loop_depth -= 1;
                self.jump_targets.pop();
                let (variable, body, scoped) = result?;
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
                let target = self.loop_target(&[]);
                self.jump_targets.push(target);
                self.loop_depth += 1;
                let result = self.for_head(fo.initializer(), fo.body(), span);
                self.loop_depth -= 1;
                self.jump_targets.pop();
                let (variable, body, scoped) = result?;
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
            Statement::Break(_) => self.jump(false, m)?,
            Statement::Continue(_) => self.jump(true, m)?,
            Statement::Return(r) => {
                let value = match r.target() {
                    Some(e) => {
                        let captured = self.hidden("__crush_return_");
                        let evaluated = self.expr(e)?;
                        let mut out = vec![self.var_decl_stmt(captured.clone(), evaluated, false, span)];
                        out.extend(self.cleanup(0));
                        out.push(CastStmt::Return { value: Some(CastExpr::Var { name: captured, meta: m.clone() }), meta: m });
                        return Ok(out);
                    }
                    None => None,
                };
                let mut out = self.cleanup(0);
                out.push(CastStmt::Return { value, meta: m });
                out
            }
            Statement::Throw(t) => {
                let value = self.expr(t.target())?;
                vec![CastStmt::Throw { value, meta: m }]
            }
            Statement::Try(t) => {
                // Runtime VmErrors are returned directly by fallible VM ops
                // and bypass CAST TryCatch (and this duplicated-finally route),
                // matching existing try/catch behavior. Typed runtime-error
                // exceptions are CRUSH-zorro-34, not this walker change.
                let saved_signal_count = self.completion_signals.len();
                let outer_try_depth = self.try_depth;
                let outer_finally_depth = self.finally_stack.len();
                let finalizer = match t.finally() {
                    Some(f) => Some(self.block(f.block().statement_list().statements())?),
                    None => None,
                };
                if let Some(body) = &finalizer {
                    self.finally_stack.push(body.clone());
                }
                self.try_depth += 1;
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
                            let handler = self.block(c.block().statement_list().statements())?;
                            Ok((var, handler))
                        })();
                        self.scopes.pop_block();
                        r?
                    }
                    None => (self.hidden("__catch_"), vec![]),
                };
                self.try_depth = outer_try_depth;
                let local_signals = self.completion_signals.split_off(saved_signal_count);
                let signals_outward: Vec<_> = local_signals
                    .iter()
                    .filter(|signal| signal.target_try_depth < outer_try_depth)
                    .cloned()
                    .collect();
                let mut user_handler = handler;
                if t.catch().is_some()
                    && let Some(condition) = self.completion_condition(&error_var, &local_signals, &m)
                {
                    user_handler.insert(0, CastStmt::If {
                        condition,
                        then_body: vec![CastStmt::Throw {
                            value: CastExpr::Var { name: error_var.clone(), meta: m.clone() },
                            meta: m.clone(),
                        }],
                        else_body: Some(vec![]),
                        meta: m.clone(),
                    });
                }
                let source_try = if t.catch().is_some() {
                    vec![CastStmt::TryCatch { body, error_var, handler: user_handler, meta: m.clone() }]
                } else {
                    body
                };
                let mut out = if finalizer.is_some() || !local_signals.is_empty() {
                    let marker = self.hidden("__crush_completion_error_");
                    let mut dispatch = self.dispatch_finally(
                        &marker,
                        finalizer.as_deref().unwrap_or(&[]),
                        outer_try_depth,
                        &local_signals,
                        &m,
                    );
                    if finalizer.is_none() {
                        // No cleanup at this level: pass the private completion
                        // through the CAST handler unchanged to the loop boundary.
                        dispatch = vec![CastStmt::Throw {
                            value: CastExpr::Var { name: marker.clone(), meta: m.clone() },
                            meta: m.clone(),
                        }];
                    }
                    vec![CastStmt::TryCatch { body: source_try, error_var: marker, handler: dispatch, meta: m.clone() }]
                } else {
                    source_try
                };
                if let Some(finally_body) = finalizer {
                    out.extend(finally_body);
                }
                self.completion_signals.extend(signals_outward);
                self.finally_stack.truncate(outer_finally_depth);
                out
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
        let mut body = self.loop_body_with_prefix(fl.body(), &update)?;
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
            // JS case matching is strict equality (CRUSH-290).
            let test = js_cap("js.strict_eq", vec![var(&val), self.expr(test)?], m.clone());
            chain = Some(vec![CastStmt::If {
                condition: test,
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
        let switch_target = super::JumpTarget {
            finally_depth: self.finally_stack.len(),
            try_depth: self.try_depth,
            is_loop: false,
            is_switch: true,
            break_action: vec![CastStmt::Break { meta: m.clone() }],
            continue_action: vec![],
        };
        self.jump_targets.push(switch_target);
        self.switch_loop_floors.push(self.loop_depth);
        let r = (|| {
            let mut syms = Vec::new();
            for c in cases {
                lexical_syms(c.body().statements(), &mut syms);
            }
            self.declare_block_syms(syms);
            let mut body = Vec::new();
            for c in cases {
                body.extend(self.hoisted_functions(c.body().statements(), Hoist::Block)?);
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
        self.switch_loop_floors.pop();
        self.jump_targets.pop();
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
            // A call's value is discarded here (`a.push(x);`).
            if let Expression::Call(c) = unparen(p)
                && let Some(stmts) = self.push_statements(c, Some(p.span()))
            {
                out.extend(stmts?);
                continue;
            }
            let expr = match unparen(p) {
                Expression::Call(c) => {
                    self.call(c.function(), c.args(), true, Some(p.span()), meta.clone())?
                }
                _ => self.expr(p)?,
            };
            out.push(CastStmt::ExprStmt { expr, meta });
        }
        Ok(out)
    }

    /// `xs.push(a, b, …);` with two or more arguments: JS evaluates every
    /// argument before appending any, so they go into hidden locals first,
    /// then each is pushed on its own (the `push` capability appends one
    /// value). `None` if `c` isn't such a call.
    fn push_statements(
        &mut self,
        c: &boa_ast::expression::Call,
        span: Option<Span>,
    ) -> Option<anyhow::Result<Vec<CastStmt>>> {
        let Expression::PropertyAccess(PropertyAccess::Simple(s)) = unparen(c.function()) else {
            return None;
        };
        let PropertyAccessField::Const(prop) = s.field() else {
            return None;
        };
        let Expression::Identifier(recv) = unparen(s.target()) else {
            return None;
        };
        if c.args().len() < 2
            || self.name(prop.sym()) != "push"
            || c.args().iter().any(|a| matches!(a, Expression::Spread(_)))
        {
            return None;
        }
        let recv = self.user_binding(recv.sym())?;
        if !matches!(
            builtins::method("push").map(|r| r.lower),
            Some(builtins::Lowering::Push)
        ) {
            return None;
        }
        Some((|| {
            let m = self.meta(span);
            let mut out = Vec::new();
            let mut temps = Vec::new();
            for a in c.args() {
                let t = self.hidden("__push_");
                let value = self.expr(a)?;
                out.push(self.var_decl_stmt(t.clone(), value, false, Some(a.span())));
                temps.push(t);
            }
            for t in temps {
                let expr = CastExpr::Call {
                    function: format!("{recv}.push"),
                    args: vec![CastExpr::Var {
                        name: t,
                        meta: m.clone(),
                    }],
                    meta: m.clone(),
                };
                out.push(CastStmt::ExprStmt {
                    expr,
                    meta: m.clone(),
                });
            }
            Ok(out)
        })())
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
