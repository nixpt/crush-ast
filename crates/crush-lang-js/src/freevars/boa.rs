//! The boa walk behind [`super::free_variables`]: node for node the same rules
//! as `swc.rs` (see `twin_tests`), over the `boa_ast` 0.22 tree.

use std::collections::HashSet;

use boa_ast::declaration::{Binding, Declaration, ImportKind, LexicalDeclaration, VarDeclaration};
use boa_ast::expression::access::{PropertyAccess, PropertyAccessField};
use boa_ast::expression::literal::{PropertyDefinition, TemplateElement};
use boa_ast::expression::operator::assign::AssignTarget;
use boa_ast::expression::operator::binary::BinaryOp;
use boa_ast::expression::Expression;
use boa_ast::statement::iteration::{ForLoopInitializer, IterableLoopInitializer};
use boa_ast::{ModuleItem, Statement, StatementListItem};
use boa_interner::{Interner, Sym};

use super::FreeVars;
use crate::backend::boa::{self as parser, BoaAst};

/// Analyze a JS source block via `boa_parser` — not a regex.
pub(super) fn free_variables(source: &str) -> Result<FreeVars, String> {
    let ast = parser::parse(source).map_err(|e| format!("{e:#}"))?;
    let (items, interner): (Vec<Item<'_>>, &Interner) = match &ast {
        BoaAst::Module(m, i) => (m.items().items().iter().map(Item::Module).collect(), i),
        BoaAst::Script(s, i) => (s.statements().iter().map(Item::Stmt).collect(), i),
    };
    let w = Walk { interner };

    let mut bound_anywhere: HashSet<String> = HashSet::new();
    for item in &items {
        w.bound_item(item, &mut bound_anywhere);
    }

    let mut reads = Reads::default();
    for item in &items {
        if let Some(sli) = item.stmt() {
            w.reads_item(sli, &mut reads);
        }
    }
    let reads = reads.seen.into_iter().filter(|n| !bound_anywhere.contains(n)).collect();

    let mut top_level_bound = Vec::new();
    for item in &items {
        match item.stmt() {
            Some(StatementListItem::Statement(s)) => match s.as_ref() {
                Statement::Var(v) => w.var_names(v, &mut top_level_bound),
                Statement::Expression(Expression::Assign(a)) => {
                    if let AssignTarget::Identifier(id) = a.lhs() {
                        top_level_bound.push(w.name(id.sym()));
                    }
                }
                _ => {}
            },
            Some(StatementListItem::Declaration(d)) => {
                if let Declaration::Lexical(lex) = d.as_ref() {
                    w.lexical_names(lex, &mut top_level_bound)
                }
            }
            None => {}
        }
    }

    Ok(FreeVars { reads, top_level_bound })
}

/// A top-level item of either a module or a script.
enum Item<'a> {
    Module(&'a ModuleItem),
    Stmt(&'a StatementListItem),
}

impl<'a> Item<'a> {
    fn stmt(&self) -> Option<&'a StatementListItem> {
        match self {
            Item::Module(ModuleItem::StatementListItem(s)) => Some(s),
            Item::Stmt(s) => Some(s),
            Item::Module(_) => None,
        }
    }
}

/// Reads in first-occurrence order, deduplicated.
#[derive(Default)]
struct Reads {
    seen: Vec<String>,
    dedup: HashSet<String>,
}

impl Reads {
    fn push(&mut self, name: String) {
        if self.dedup.insert(name.clone()) {
            self.seen.push(name);
        }
    }
}

struct Walk<'i> {
    interner: &'i Interner,
}

impl Walk<'_> {
    fn name(&self, sym: Sym) -> String {
        self.interner.resolve_expect(sym).to_string()
    }

    fn binding_name(&self, b: &Binding) -> Option<String> {
        match b {
            Binding::Identifier(id) => Some(self.name(id.sym())),
            Binding::Pattern(_) => None,
        }
    }

    fn var_names(&self, v: &VarDeclaration, out: &mut Vec<String>) {
        out.extend(v.0.as_ref().iter().filter_map(|d| self.binding_name(d.binding())));
    }

    fn lexical_names(&self, lex: &LexicalDeclaration, out: &mut Vec<String>) {
        out.extend(lex.variable_list().as_ref().iter().filter_map(|d| self.binding_name(d.binding())));
    }

    // ── bound names ─────────────────────────────────────────────────────────

    fn bound_item(&self, item: &Item<'_>, bound: &mut HashSet<String>) {
        match item {
            Item::Module(ModuleItem::ImportDeclaration(import)) => {
                if let Some(d) = import.default() {
                    bound.insert(self.name(d.sym()));
                }
                match import.kind() {
                    ImportKind::DefaultOrUnnamed => {}
                    ImportKind::Namespaced { binding } => {
                        bound.insert(self.name(binding.sym()));
                    }
                    ImportKind::Named { names } => {
                        for spec in names {
                            bound.insert(self.name(spec.binding().sym()));
                        }
                    }
                }
            }
            Item::Module(ModuleItem::ExportDeclaration(_)) => {}
            _ => {
                if let Some(sli) = item.stmt() {
                    self.bound_sli(sli, bound);
                }
            }
        }
    }

    fn bound_sli(&self, sli: &StatementListItem, bound: &mut HashSet<String>) {
        match sli {
            StatementListItem::Statement(s) => self.bound_stmt(s, bound),
            StatementListItem::Declaration(d) => self.bound_decl(d, bound),
        }
    }

    fn bound_list(&self, items: &[StatementListItem], bound: &mut HashSet<String>) {
        for s in items {
            self.bound_sli(s, bound);
        }
    }

    fn bound_decl(&self, decl: &Declaration, bound: &mut HashSet<String>) {
        let mut names = Vec::new();
        match decl {
            Declaration::Lexical(lex) => self.lexical_names(lex, &mut names),
            Declaration::FunctionDeclaration(f) => names.push(self.name(f.name().sym())),
            Declaration::GeneratorDeclaration(f) => names.push(self.name(f.name().sym())),
            Declaration::AsyncFunctionDeclaration(f) => names.push(self.name(f.name().sym())),
            Declaration::AsyncGeneratorDeclaration(f) => names.push(self.name(f.name().sym())),
            Declaration::ClassDeclaration(c) => names.push(self.name(c.name().sym())),
        }
        bound.extend(names);
    }

    fn bound_stmt(&self, stmt: &Statement, bound: &mut HashSet<String>) {
        match stmt {
            Statement::Var(v) => {
                let mut names = Vec::new();
                self.var_names(v, &mut names);
                bound.extend(names);
            }
            Statement::ForLoop(fl) => {
                let mut names = Vec::new();
                match fl.init() {
                    Some(ForLoopInitializer::Var(v)) => self.var_names(v, &mut names),
                    Some(ForLoopInitializer::Lexical(l)) => self.lexical_names(l.declaration(), &mut names),
                    _ => {}
                }
                bound.extend(names);
                self.bound_stmt(fl.body(), bound);
            }
            Statement::ForInLoop(f) => {
                self.bound_iter_init(f.initializer(), bound);
                self.bound_stmt(f.body(), bound);
            }
            Statement::ForOfLoop(f) => {
                self.bound_iter_init(f.initializer(), bound);
                self.bound_stmt(f.body(), bound);
            }
            Statement::WhileLoop(w) => self.bound_stmt(w.body(), bound),
            Statement::DoWhileLoop(w) => self.bound_stmt(w.body(), bound),
            Statement::If(i) => {
                self.bound_stmt(i.body(), bound);
                if let Some(alt) = i.else_node() {
                    self.bound_stmt(alt, bound);
                }
            }
            Statement::Block(b) => self.bound_list(b.statement_list().statements(), bound),
            Statement::Try(t) => {
                self.bound_list(t.block().statement_list().statements(), bound);
                if let Some(c) = t.catch() {
                    if let Some(name) = c.parameter().and_then(|p| self.binding_name(p)) {
                        bound.insert(name);
                    }
                    self.bound_list(c.block().statement_list().statements(), bound);
                }
                if let Some(f) = t.finally() {
                    self.bound_list(f.block().statement_list().statements(), bound);
                }
            }
            _ => {}
        }
    }

    fn bound_iter_init(&self, init: &IterableLoopInitializer, bound: &mut HashSet<String>) {
        match init {
            IterableLoopInitializer::Var(v) => {
                if let Some(n) = self.binding_name(v.binding()) {
                    bound.insert(n);
                }
            }
            IterableLoopInitializer::Let(b) | IterableLoopInitializer::Const(b) => {
                if let Some(n) = self.binding_name(b) {
                    bound.insert(n);
                }
            }
            _ => {}
        }
    }

    // ── reads ───────────────────────────────────────────────────────────────

    fn reads_item(&self, sli: &StatementListItem, reads: &mut Reads) {
        let decl = match sli {
            StatementListItem::Statement(s) => return self.reads_stmt(s, reads),
            StatementListItem::Declaration(d) => d.as_ref(),
        };
        match decl {
            Declaration::Lexical(lex) => {
                for d in lex.variable_list().as_ref() {
                    if let Some(init) = d.init() {
                        self.reads_expr(init, reads);
                    }
                }
            }
            // swc's `Decl::Fn` covers async and generator functions too.
            Declaration::FunctionDeclaration(f) => self.reads_list(f.body().statements(), reads),
            Declaration::GeneratorDeclaration(f) => self.reads_list(f.body().statements(), reads),
            Declaration::AsyncFunctionDeclaration(f) => self.reads_list(f.body().statements(), reads),
            Declaration::AsyncGeneratorDeclaration(f) => self.reads_list(f.body().statements(), reads),
            Declaration::ClassDeclaration(_) => {}
        }
    }

    fn reads_list(&self, items: &[StatementListItem], reads: &mut Reads) {
        for s in items {
            self.reads_item(s, reads);
        }
    }

    fn reads_stmt(&self, stmt: &Statement, reads: &mut Reads) {
        match stmt {
            Statement::Expression(e) => self.reads_expr(e, reads),
            Statement::Var(v) => {
                for d in v.0.as_ref() {
                    if let Some(init) = d.init() {
                        self.reads_expr(init, reads);
                    }
                }
            }
            Statement::Return(r) => {
                if let Some(e) = r.target() {
                    self.reads_expr(e, reads);
                }
            }
            Statement::If(i) => {
                self.reads_expr(i.cond(), reads);
                self.reads_stmt(i.body(), reads);
                if let Some(alt) = i.else_node() {
                    self.reads_stmt(alt, reads);
                }
            }
            Statement::WhileLoop(w) => {
                self.reads_expr(w.condition(), reads);
                self.reads_stmt(w.body(), reads);
            }
            Statement::Block(b) => self.reads_list(b.statement_list().statements(), reads),
            Statement::ForLoop(fl) => {
                match fl.init() {
                    Some(ForLoopInitializer::Expression(e)) => self.reads_expr(e, reads),
                    Some(ForLoopInitializer::Var(v)) => {
                        for d in v.0.as_ref() {
                            if let Some(init) = d.init() {
                                self.reads_expr(init, reads);
                            }
                        }
                    }
                    Some(ForLoopInitializer::Lexical(l)) => {
                        for d in l.declaration().variable_list().as_ref() {
                            if let Some(init) = d.init() {
                                self.reads_expr(init, reads);
                            }
                        }
                    }
                    None => {}
                }
                if let Some(t) = fl.condition() {
                    self.reads_expr(t, reads);
                }
                if let Some(u) = fl.final_expr() {
                    self.reads_expr(u, reads);
                }
                self.reads_stmt(fl.body(), reads);
            }
            _ => {}
        }
    }

    /// An array element or call argument: swc reads through a spread there.
    fn reads_elem(&self, e: &Expression, reads: &mut Reads) {
        match e {
            Expression::Spread(s) => self.reads_expr(s.target(), reads),
            e => self.reads_expr(e, reads),
        }
    }

    fn reads_access(&self, pa: &PropertyAccess, reads: &mut Reads) {
        match pa {
            PropertyAccess::Simple(s) => {
                self.reads_expr(s.target(), reads);
                if let PropertyAccessField::Expr(c) = s.field() {
                    self.reads_expr(c, reads);
                }
            }
            PropertyAccess::Private(p) => self.reads_expr(p.target(), reads),
            PropertyAccess::Super(_) => {}
        }
    }

    fn reads_expr(&self, expr: &Expression, reads: &mut Reads) {
        match expr {
            Expression::Identifier(id) => reads.push(self.name(id.sym())),
            // swc's comma expression is `Expr::Seq`, which it doesn't walk.
            Expression::Binary(b) if b.op() != BinaryOp::Comma => {
                self.reads_expr(b.lhs(), reads);
                self.reads_expr(b.rhs(), reads);
            }
            Expression::Unary(u) => self.reads_expr(u.target(), reads),
            Expression::Assign(a) => {
                self.reads_expr(a.rhs(), reads);
                // An identifier target is a bind, not a read.
                if let AssignTarget::Access(pa) = a.lhs() {
                    self.reads_access(pa, reads);
                }
            }
            Expression::Call(c) => {
                self.reads_expr(c.function(), reads);
                for a in c.args() {
                    self.reads_elem(a, reads);
                }
            }
            Expression::PropertyAccess(pa) => self.reads_access(pa, reads),
            Expression::ArrayLiteral(a) => {
                for e in a.as_ref().iter().flatten() {
                    self.reads_elem(e, reads);
                }
            }
            Expression::ObjectLiteral(o) => {
                for p in o.properties() {
                    if let PropertyDefinition::Property(_, value) = p {
                        self.reads_expr(value, reads);
                    }
                }
            }
            Expression::Parenthesized(p) => self.reads_expr(p.expression(), reads),
            Expression::TemplateLiteral(t) => {
                for el in t.elements() {
                    if let TemplateElement::Expr(e) = el {
                        self.reads_expr(e, reads);
                    }
                }
            }
            Expression::Conditional(c) => {
                self.reads_expr(c.condition(), reads);
                self.reads_expr(c.if_true(), reads);
                self.reads_expr(c.if_false(), reads);
            }
            _ => {}
        }
    }
}
