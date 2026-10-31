//! The scope pass: which crush local a JS name refers to.
//!
//! Crush locals are keyed by name, one namespace per crush function (the
//! compiler doesn't scope `if`/loop bodies), and a lambda sees its enclosing
//! function's locals by name (captures). So:
//!
//! - **Function-level bindings** (parameters, every `var` in the body, the
//!   body's top-level `let`/`const`/`function`) keep their JS names. They are
//!   declared when the function is entered, before its statements, so a use
//!   ahead of the declaration (hoisting, a closure) finds them.
//! - **Block-level bindings** (a nested `let`/`const`/`function`, a `for`
//!   head's `let`, a `catch` parameter) get a fresh name nothing else in the
//!   program uses. Shadowing, and a block binding next to a global of the same
//!   name, then can't collide. They too are declared on entry to their block.
//! - A name no frame declares is a global (`console`, `Math`, a top-level
//!   function) and keeps its spelling.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use boa_ast::declaration::{Binding, Declaration, LexicalDeclaration, VarDeclaration, Variable};
use boa_ast::function::FormalParameterList;
use boa_ast::pattern::{ArrayPatternElement, ObjectPatternElement, Pattern};
use boa_ast::statement::iteration::{ForLoopInitializer, IterableLoopInitializer};
use boa_ast::{Statement, StatementListItem};
use boa_interner::Sym;

#[derive(Default)]
pub(super) struct Scopes {
    /// One entry per crush function being lowered, innermost last.
    fns: Vec<FnScope>,
}

#[derive(Default)]
pub(super) struct FnScope {
    /// `frames[0]` is the function level; the rest are blocks, innermost last.
    frames: Vec<HashMap<Sym, String>>,
    /// Per frame: the `let`/`const` bindings whose cell is created on entry
    /// to the frame (see `Lower::predeclare`); their declaration assigns.
    predeclared: Vec<HashSet<Sym>>,
    /// Per frame: the parameter kinds of the `function` declarations bound
    /// there, so their calls can be adapted (CRUSH-zorro-18).
    sigs: Vec<HashMap<Sym, Sig>>,
    /// Per frame: the bindings that are class declarations (or bindings
    /// of a class expression), so a use can tell the class binding from
    /// a parameter/local of the same spelling that shadows it
    /// (CRUSH-281 review).
    classes: Vec<HashSet<Sym>>,
}

/// How a `function` declaration's parameter is passed (CRUSH-zorro-18).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Param {
    /// `x`: the argument, which must be given (a missing one is `undefined`).
    Plain,
    /// `x = …`: the argument or a placeholder, plus a hidden "given" flag
    /// appended after the parameters; the callee evaluates the default when
    /// the flag is false.
    Default,
    /// `...xs`: the remaining arguments as one array.
    Rest,
}

/// A `function` declaration's parameters.
pub(super) type Sig = Rc<Vec<Param>>;

/// The parameter kinds of `params`.
pub(super) fn sig_of(params: &FormalParameterList) -> Sig {
    Rc::new(
        params
            .as_ref()
            .iter()
            .map(|p| {
                if p.is_rest_param() {
                    Param::Rest
                } else if p.init().is_some() {
                    Param::Default
                } else {
                    Param::Plain
                }
            })
            .collect(),
    )
}

impl Scopes {
    /// Start a crush function nested in the current one (a lambda: it sees
    /// the enclosing locals).
    pub(super) fn enter_fn(&mut self) {
        self.fns.push(FnScope {
            frames: vec![HashMap::new()],
            predeclared: vec![HashSet::new()],
            sigs: vec![HashMap::new()],
            classes: vec![HashSet::new()],
        });
    }

    pub(super) fn leave_fn(&mut self) {
        self.fns.pop();
    }

    /// Hide every enclosing scope, for a crush function that sees no enclosing
    /// locals (a top-level `function`). Returns what to give back to
    /// [`Scopes::restore`].
    pub(super) fn isolate(&mut self) -> Vec<FnScope> {
        std::mem::take(&mut self.fns)
    }

    pub(super) fn restore(&mut self, saved: Vec<FnScope>) {
        self.fns = saved;
    }

    fn current(&mut self) -> &mut FnScope {
        self.fns.last_mut().expect("inside a function")
    }

    /// Declare a function-level binding (keeps its name).
    pub(super) fn declare_root(&mut self, sym: Sym, name: String) {
        self.current().frames[0].insert(sym, name);
    }

    pub(super) fn push_block(&mut self) {
        let f = self.current();
        f.frames.push(HashMap::new());
        f.predeclared.push(HashSet::new());
        f.sigs.push(HashMap::new());
        f.classes.push(HashSet::new());
    }

    pub(super) fn pop_block(&mut self) {
        let f = self.current();
        debug_assert!(f.frames.len() > 1, "pop_block without push_block");
        f.frames.pop();
        f.predeclared.pop();
        f.sigs.pop();
        f.classes.pop();
    }

    /// Mark the binding `sym` refers to in its declaring frame as a
    /// class binding. No-op when no frame declares `sym` (a class
    /// expression's own name, bound only inside the class, is such a
    /// name — it must not mark an unrelated outer binding).
    pub(super) fn mark_class(&mut self, sym: Sym) {
        for f in self.fns.iter_mut().rev() {
            for (frame, classes) in f.frames.iter().zip(&mut f.classes).rev() {
                if frame.contains_key(&sym) {
                    classes.insert(sym);
                    return;
                }
            }
        }
    }

    /// Is the binding `sym` refers to a class binding? `Some(true)` for
    /// the class's own binding, `Some(false)` when the nearest binding
    /// of that spelling is something else (a parameter, a `let`, a
    /// function, an import) that shadows any class, and `None` when no
    /// visible frame declares it (a global, or a module binding seen
    /// from an isolated top-level function).
    pub(super) fn binding_class(&self, sym: Sym) -> Option<bool> {
        for f in self.fns.iter().rev() {
            for (frame, classes) in f.frames.iter().zip(&f.classes).rev() {
                if frame.contains_key(&sym) {
                    return Some(classes.contains(&sym));
                }
            }
        }
        None
    }

    /// Declare (or rebind) a binding of the innermost frame under `name`.
    pub(super) fn declare_block(&mut self, sym: Sym, name: String) {
        self.current()
            .frames
            .last_mut()
            .expect("a frame")
            .insert(sym, name);
    }

    /// Mark a binding of the innermost frame as created on entry.
    pub(super) fn predeclare(&mut self, sym: Sym) {
        self.current()
            .predeclared
            .last_mut()
            .expect("a frame")
            .insert(sym);
    }

    /// Was this binding of the innermost frame created on entry?
    pub(super) fn is_predeclared(&self, sym: Sym) -> bool {
        self.fns
            .last()
            .and_then(|f| f.predeclared.last())
            .is_some_and(|p| p.contains(&sym))
    }

    /// Record the parameters of the `function` bound to `sym` in the innermost frame.
    pub(super) fn set_sig(&mut self, sym: Sym, sig: Sig) {
        self.current()
            .sigs
            .last_mut()
            .expect("a frame")
            .insert(sym, sig);
    }

    /// The parameters of the `function` declaration `sym` refers to, if it
    /// refers to one (a shadowing binding without a signature hides it).
    pub(super) fn resolve_sig(&self, sym: Sym) -> Option<Sig> {
        for f in self.fns.iter().rev() {
            for (frame, sigs) in f.frames.iter().zip(&f.sigs).rev() {
                if frame.contains_key(&sym) {
                    return sigs.get(&sym).cloned();
                }
            }
        }
        None
    }

    /// The crush local `sym` refers to, if any frame declares it.
    pub(super) fn resolve(&self, sym: Sym) -> Option<&str> {
        self.fns
            .iter()
            .rev()
            .flat_map(|f| f.frames.iter().rev())
            .find_map(|frame| frame.get(&sym))
            .map(String::as_str)
    }
}

// ── What a function or block declares ───────────────────────────────────────

/// Names a binding introduces (patterns included, for scoping only: the
/// lowering refuses destructuring itself).
pub(super) fn binding_syms(b: &Binding, out: &mut Vec<Sym>) {
    match b {
        Binding::Identifier(id) => out.push(id.sym()),
        Binding::Pattern(p) => pattern_syms(p, out),
    }
}

fn pattern_syms(p: &Pattern, out: &mut Vec<Sym>) {
    match p {
        Pattern::Object(o) => {
            for el in o.bindings() {
                match el {
                    ObjectPatternElement::SingleName { ident, .. } => out.push(ident.sym()),
                    ObjectPatternElement::RestProperty { ident } => out.push(ident.sym()),
                    ObjectPatternElement::Pattern { pattern, .. } => pattern_syms(pattern, out),
                    // Assignment patterns name existing targets, not new bindings.
                    ObjectPatternElement::AssignmentPropertyAccess { .. }
                    | ObjectPatternElement::AssignmentRestPropertyAccess { .. } => {}
                }
            }
        }
        Pattern::Array(a) => {
            for el in a.bindings() {
                match el {
                    ArrayPatternElement::SingleName { ident, .. } => out.push(ident.sym()),
                    ArrayPatternElement::SingleNameRest { ident } => out.push(ident.sym()),
                    ArrayPatternElement::Pattern { pattern, .. }
                    | ArrayPatternElement::PatternRest { pattern } => pattern_syms(pattern, out),
                    ArrayPatternElement::Elision
                    | ArrayPatternElement::PropertyAccess { .. }
                    | ArrayPatternElement::PropertyAccessRest { .. } => {}
                }
            }
        }
    }
}

pub(super) fn param_syms(params: &FormalParameterList, out: &mut Vec<Sym>) {
    for p in params.as_ref() {
        binding_syms(p.variable().binding(), out);
    }
}

fn var_decl_syms(v: &VarDeclaration, out: &mut Vec<Sym>) {
    for d in v.0.as_ref() {
        binding_syms(d.binding(), out);
    }
}

/// Every `var` in `items`, at any depth, but not inside nested functions.
pub(super) fn var_syms(items: &[StatementListItem], out: &mut Vec<Sym>) {
    for item in items {
        if let StatementListItem::Statement(s) = item {
            var_syms_stmt(s, out);
        }
    }
}

fn var_syms_stmt(s: &Statement, out: &mut Vec<Sym>) {
    match s {
        Statement::Var(v) => var_decl_syms(v, out),
        Statement::Block(b) => var_syms(b.statement_list().statements(), out),
        Statement::If(i) => {
            var_syms_stmt(i.body(), out);
            if let Some(e) = i.else_node() {
                var_syms_stmt(e, out);
            }
        }
        Statement::WhileLoop(w) => var_syms_stmt(w.body(), out),
        Statement::DoWhileLoop(w) => var_syms_stmt(w.body(), out),
        Statement::ForLoop(f) => {
            if let Some(ForLoopInitializer::Var(v)) = f.init() {
                var_decl_syms(v, out);
            }
            var_syms_stmt(f.body(), out);
        }
        Statement::ForInLoop(f) => {
            if let IterableLoopInitializer::Var(v) = f.initializer() {
                binding_syms(v.binding(), out);
            }
            var_syms_stmt(f.body(), out);
        }
        Statement::ForOfLoop(f) => {
            if let IterableLoopInitializer::Var(v) = f.initializer() {
                binding_syms(v.binding(), out);
            }
            var_syms_stmt(f.body(), out);
        }
        Statement::Try(t) => {
            var_syms(t.block().statement_list().statements(), out);
            if let Some(c) = t.catch() {
                var_syms(c.block().statement_list().statements(), out);
            }
            if let Some(f) = t.finally() {
                var_syms(f.block().statement_list().statements(), out);
            }
        }
        Statement::Switch(sw) => {
            for c in sw.cases() {
                var_syms(c.body().statements(), out);
            }
        }
        Statement::Labelled(l) => {
            if let boa_ast::statement::LabelledItem::Statement(s) = l.item() {
                var_syms_stmt(s, out);
            }
        }
        Statement::With(w) => var_syms_stmt(w.statement(), out),
        _ => {}
    }
}

/// Every binding assigned anywhere in `items` — assignments and updates
/// in expressions included, nested function bodies included (their
/// assignments are exactly what the caller wants to forget about). The
/// loop back-edge rule (CRUSH-286 re-review): a binding assigned
/// anywhere in a loop statement must not be a known instance while the
/// loop is lowered, because from the second iteration on it may hold
/// the assigned value instead.
pub(super) fn assigned_syms_items(items: &[StatementListItem], out: &mut Vec<Sym>) {
    for item in items {
        match item {
            StatementListItem::Statement(s) => assigned_syms_stmt(s, out),
            StatementListItem::Declaration(d) => match d.as_ref() {
                Declaration::Lexical(lex) => {
                    for v in lex.variable_list().as_ref() {
                        binding_syms(v.binding(), out);
                        if let Some(init) = v.init() {
                            assigned_syms_expr(init, out);
                        }
                    }
                }
                Declaration::FunctionDeclaration(f) => {
                    assigned_syms_items(f.body().statements(), out);
                }
                _ => {}
            },
        }
    }
}

pub(super) fn assigned_syms_stmt(s: &Statement, out: &mut Vec<Sym>) {
    match s {
        Statement::Block(b) => assigned_syms_items(b.statement_list().statements(), out),
        Statement::Expression(e) => assigned_syms_expr(e, out),
        Statement::Var(v) => {
            for d in v.0.as_ref() {
                binding_syms(d.binding(), out);
                if let Some(init) = d.init() {
                    assigned_syms_expr(init, out);
                }
            }
        }
        Statement::If(i) => {
            assigned_syms_expr(i.cond(), out);
            assigned_syms_stmt(i.body(), out);
            if let Some(e) = i.else_node() {
                assigned_syms_stmt(e, out);
            }
        }
        Statement::WhileLoop(w) => {
            assigned_syms_expr(w.condition(), out);
            assigned_syms_stmt(w.body(), out);
        }
        Statement::DoWhileLoop(w) => {
            assigned_syms_stmt(w.body(), out);
            assigned_syms_expr(w.cond(), out);
        }
        Statement::ForLoop(f) => {
            match f.init() {
                Some(ForLoopInitializer::Var(v)) => {
                    for d in v.0.as_ref() {
                        binding_syms(d.binding(), out);
                        if let Some(init) = d.init() {
                            assigned_syms_expr(init, out);
                        }
                    }
                }
                Some(ForLoopInitializer::Lexical(l)) => {
                    for d in l.declaration().variable_list().as_ref() {
                        binding_syms(d.binding(), out);
                        if let Some(init) = d.init() {
                            assigned_syms_expr(init, out);
                        }
                    }
                }
                Some(ForLoopInitializer::Expression(e)) => assigned_syms_expr(e, out),
                None => {}
            }
            if let Some(c) = f.condition() {
                assigned_syms_expr(c, out);
            }
            if let Some(u) = f.final_expr() {
                assigned_syms_expr(u, out);
            }
            assigned_syms_stmt(f.body(), out);
        }
        Statement::ForInLoop(f) => {
            match f.initializer() {
                IterableLoopInitializer::Identifier(id) => out.push(id.sym()),
                IterableLoopInitializer::Var(v) => binding_syms(v.binding(), out),
                IterableLoopInitializer::Let(b) | IterableLoopInitializer::Const(b) => {
                    binding_syms(b, out);
                }
                _ => {}
            }
            assigned_syms_expr(f.target(), out);
            assigned_syms_stmt(f.body(), out);
        }
        Statement::ForOfLoop(f) => {
            match f.initializer() {
                IterableLoopInitializer::Identifier(id) => out.push(id.sym()),
                IterableLoopInitializer::Var(v) => binding_syms(v.binding(), out),
                IterableLoopInitializer::Let(b) | IterableLoopInitializer::Const(b) => {
                    binding_syms(b, out);
                }
                _ => {}
            }
            assigned_syms_expr(f.iterable(), out);
            assigned_syms_stmt(f.body(), out);
        }
        Statement::Switch(sw) => {
            assigned_syms_expr(sw.val(), out);
            for c in sw.cases() {
                if let Some(t) = c.condition() {
                    assigned_syms_expr(t, out);
                }
                assigned_syms_items(c.body().statements(), out);
            }
        }
        Statement::Try(t) => {
            assigned_syms_items(t.block().statement_list().statements(), out);
            if let Some(c) = t.catch() {
                assigned_syms_items(c.block().statement_list().statements(), out);
            }
            if let Some(f) = t.finally() {
                assigned_syms_items(f.block().statement_list().statements(), out);
            }
        }
        Statement::Return(r) => {
            if let Some(e) = r.target() {
                assigned_syms_expr(e, out);
            }
        }
        Statement::Throw(t) => assigned_syms_expr(t.target(), out),
        Statement::Labelled(l) => {
            if let boa_ast::statement::LabelledItem::Statement(s) = l.item() {
                assigned_syms_stmt(s, out);
            }
        }
        Statement::With(w) => {
            assigned_syms_expr(w.expression(), out);
            assigned_syms_stmt(w.statement(), out);
        }
        _ => {}
    }
}

fn assigned_syms_expr(e: &boa_ast::expression::Expression, out: &mut Vec<Sym>) {
    use boa_ast::expression::Expression;
    use boa_ast::expression::access::{PropertyAccess, PropertyAccessField};
    use boa_ast::expression::literal::{PropertyDefinition, TemplateElement};
    use boa_ast::expression::operator::assign::AssignTarget;
    use boa_ast::expression::operator::update::UpdateTarget;
    match e {
        Expression::Assign(a) => {
            match a.lhs() {
                AssignTarget::Identifier(id) => out.push(id.sym()),
                AssignTarget::Access(PropertyAccess::Simple(s)) => {
                    assigned_syms_expr(s.target(), out);
                    if let PropertyAccessField::Expr(f) = s.field() {
                        assigned_syms_expr(f, out);
                    }
                }
                _ => {}
            }
            assigned_syms_expr(a.rhs(), out);
        }
        Expression::Update(u) => match u.target() {
            UpdateTarget::Identifier(id) => out.push(id.sym()),
            UpdateTarget::PropertyAccess(PropertyAccess::Simple(s)) => {
                assigned_syms_expr(s.target(), out);
            }
            _ => {}
        },
        Expression::Binary(b) => {
            assigned_syms_expr(b.lhs(), out);
            assigned_syms_expr(b.rhs(), out);
        }
        Expression::Conditional(c) => {
            assigned_syms_expr(c.condition(), out);
            assigned_syms_expr(c.if_true(), out);
            assigned_syms_expr(c.if_false(), out);
        }
        Expression::Unary(u) => assigned_syms_expr(u.target(), out),
        Expression::Parenthesized(p) => assigned_syms_expr(p.expression(), out),
        Expression::Call(c) => {
            assigned_syms_expr(c.function(), out);
            for a in c.args() {
                assigned_syms_expr(a, out);
            }
        }
        Expression::New(n) => {
            assigned_syms_expr(n.constructor(), out);
            for a in n.arguments() {
                assigned_syms_expr(a, out);
            }
        }
        Expression::PropertyAccess(PropertyAccess::Simple(s)) => {
            assigned_syms_expr(s.target(), out);
            if let PropertyAccessField::Expr(f) = s.field() {
                assigned_syms_expr(f, out);
            }
        }
        Expression::ArrayLiteral(a) => {
            for el in a.as_ref().iter().flatten() {
                assigned_syms_expr(el, out);
            }
        }
        Expression::ObjectLiteral(o) => {
            for p in o.properties() {
                match p {
                    PropertyDefinition::Property(_, v) => assigned_syms_expr(v, out),
                    PropertyDefinition::SpreadObject(v) => assigned_syms_expr(v, out),
                    _ => {}
                }
            }
        }
        Expression::TemplateLiteral(t) => {
            for el in t.elements() {
                if let TemplateElement::Expr(e) = el {
                    assigned_syms_expr(e, out);
                }
            }
        }
        Expression::Spread(s) => assigned_syms_expr(s.target(), out),
        Expression::Optional(o) => {
            use boa_ast::expression::OptionalOperationKind;
            assigned_syms_expr(o.target(), out);
            for op in o.chain() {
                match op.kind() {
                    OptionalOperationKind::Call { args } => {
                        for a in args {
                            assigned_syms_expr(a, out);
                        }
                    }
                    OptionalOperationKind::SimplePropertyAccess {
                        field: PropertyAccessField::Expr(f),
                    } => assigned_syms_expr(f, out),
                    _ => {}
                }
            }
        }
        Expression::ArrowFunction(f) => assigned_syms_items(f.body().statements(), out),
        Expression::FunctionExpression(f) => assigned_syms_items(f.body().statements(), out),
        _ => {}
    }
}

/// Every `var x;` declarator without an initializer in `items`, at any depth,
/// but not inside nested functions.
pub(super) fn uninit_var_syms(items: &[StatementListItem], out: &mut Vec<Sym>) {
    let mut vars = Vec::new();
    var_decls(items, &mut vars);
    for v in vars {
        if v.init().is_none() {
            binding_syms(v.binding(), out);
        }
    }
}

fn var_decls<'a>(items: &'a [StatementListItem], out: &mut Vec<&'a Variable>) {
    for item in items {
        if let StatementListItem::Statement(s) = item {
            var_decls_stmt(s, out);
        }
    }
}

fn var_decls_stmt<'a>(s: &'a Statement, out: &mut Vec<&'a Variable>) {
    match s {
        Statement::Var(v) => out.extend(v.0.as_ref()),
        Statement::Block(b) => var_decls(b.statement_list().statements(), out),
        Statement::If(i) => {
            var_decls_stmt(i.body(), out);
            if let Some(e) = i.else_node() {
                var_decls_stmt(e, out);
            }
        }
        Statement::WhileLoop(w) => var_decls_stmt(w.body(), out),
        Statement::DoWhileLoop(w) => var_decls_stmt(w.body(), out),
        Statement::ForLoop(f) => {
            if let Some(ForLoopInitializer::Var(v)) = f.init() {
                out.extend(v.0.as_ref());
            }
            var_decls_stmt(f.body(), out);
        }
        Statement::ForInLoop(f) => var_decls_stmt(f.body(), out),
        Statement::ForOfLoop(f) => var_decls_stmt(f.body(), out),
        Statement::Try(t) => {
            var_decls(t.block().statement_list().statements(), out);
            if let Some(c) = t.catch() {
                var_decls(c.block().statement_list().statements(), out);
            }
            if let Some(f) = t.finally() {
                var_decls(f.block().statement_list().statements(), out);
            }
        }
        Statement::Switch(sw) => {
            for c in sw.cases() {
                var_decls(c.body().statements(), out);
            }
        }
        Statement::Labelled(l) => {
            if let boa_ast::statement::LabelledItem::Statement(s) = l.item() {
                var_decls_stmt(s, out);
            }
        }
        Statement::With(w) => var_decls_stmt(w.statement(), out),
        _ => {}
    }
}

/// The block-scoped names a statement list declares directly: `let`/`const`,
/// classes and function declarations.
pub(super) fn lexical_syms(items: &[StatementListItem], out: &mut Vec<Sym>) {
    for item in items {
        if let StatementListItem::Declaration(d) = item {
            match d.as_ref() {
                Declaration::Lexical(lex) => lexical_decl_syms(lex, out),
                Declaration::ClassDeclaration(c) => out.push(c.name().sym()),
                Declaration::FunctionDeclaration(f) => out.push(f.name().sym()),
                Declaration::GeneratorDeclaration(f) => out.push(f.name().sym()),
                Declaration::AsyncFunctionDeclaration(f) => out.push(f.name().sym()),
                Declaration::AsyncGeneratorDeclaration(f) => out.push(f.name().sym()),
            }
        }
    }
}

pub(super) fn lexical_decl_syms(lex: &LexicalDeclaration, out: &mut Vec<Sym>) {
    for d in lex.variable_list().as_ref() {
        binding_syms(d.binding(), out);
    }
}
