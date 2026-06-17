use anyhow;
use boa_ast::declaration::{Declaration, LexicalDeclaration};
use boa_ast::expression::access::PropertyAccess;
use boa_ast::expression::literal::PropertyDefinition;
use boa_ast::expression::operator::assign::AssignTarget;
use boa_ast::expression::operator::update::UpdateTarget;
use boa_ast::expression::Expression;
use boa_ast::{ModuleItem, Statement, StatementListItem};
use boa_interner::{Interner, Sym};
use walker_core::FeatureReport;

use crate::backend::boa::BoaAst;

pub fn analyze(ast: &BoaAst, r: &mut FeatureReport) -> anyhow::Result<()> {
    match ast {
        BoaAst::Script(script, interner) => {
            let analyzer = BoaAnalyzer::new(interner);
            for item in script.statements().statements() {
                analyzer.walk_statement_list_item(item, r);
            }
        }
        BoaAst::Module(module, interner) => {
            let analyzer = BoaAnalyzer::new(interner);
            for item in module.items().items() {
                match item {
                    ModuleItem::ImportDeclaration(import) => {
                        let src_sym = import.specifier().sym();
                        let src = analyzer.sym_str(src_sym);
                        r.uses_imports.push(src.clone());
                        if is_dangerous_import(&src) {
                            r.dangerous_imports.push(src);
                        }
                    }
                    ModuleItem::ExportDeclaration(_) => {
                        r.uses_imports.push("export".to_string());
                    }
                    ModuleItem::StatementListItem(sli) => {
                        analyzer.walk_statement_list_item(sli, r);
                    }
                }
                r.estimated_complexity += 1;
            }
        }
    }
    Ok(())
}

fn is_dangerous_import(module: &str) -> bool {
    let dangerous = [
        "child_process", "fs", "net", "dgram", "cluster", "vm",
        "worker_threads", "os", "process", "module", "electron",
    ];
    let base = module.split('/').next().unwrap_or(module);
    dangerous.contains(&base)
}

pub struct BoaAnalyzer<'a> {
    pub interner: &'a Interner,
}

impl<'a> BoaAnalyzer<'a> {
    pub fn new(interner: &'a Interner) -> Self {
        Self { interner }
    }

    fn sym_str(&self, sym: Sym) -> String {
        self.interner.resolve_expect(sym).to_string()
    }

    fn walk_statement_list(&self, items: &[StatementListItem], r: &mut FeatureReport) {
        for item in items {
            self.walk_statement_list_item(item, r);
        }
    }

    pub fn walk_statement_list_item(&self, item: &StatementListItem, r: &mut FeatureReport) {
        match item {
            StatementListItem::Statement(stmt) => self.walk_statement(stmt, r),
            StatementListItem::Declaration(decl) => self.walk_declaration(decl, r),
        }
    }

    fn walk_statement(&self, stmt: &Statement, r: &mut FeatureReport) {
        match stmt {
            Statement::Block(block) => {
                self.walk_statement_list(block.statement_list().statements(), r);
            }
            Statement::If(if_stmt) => {
                self.walk_expression(if_stmt.cond(), r);
                self.walk_statement(if_stmt.body(), r);
                if let Some(alt) = if_stmt.else_node() {
                    self.walk_statement(alt, r);
                }
            }
            Statement::WhileLoop(w) => {
                self.walk_expression(w.condition(), r);
                self.walk_statement(w.body(), r);
            }
            Statement::DoWhileLoop(dw) => {
                self.walk_expression(dw.cond(), r);
                self.walk_statement(dw.body(), r);
            }
            Statement::ForLoop(fl) => {
                self.walk_statement(fl.body(), r);
            }
            Statement::ForInLoop(fi) => {
                self.walk_expression(fi.target(), r);
                self.walk_statement(fi.body(), r);
            }
            Statement::ForOfLoop(fo) => {
                if fo.r#await() {
                    r.uses_async = true;
                }
                self.walk_statement(fo.body(), r);
            }
            Statement::Switch(s) => {
                self.walk_expression(s.val(), r);
                for case in s.cases() {
                    self.walk_statement_list(case.body().statements(), r);
                }
            }
            Statement::Try(try_stmt) => {
                r.uses_exceptions = true;
                self.walk_statement_list(try_stmt.block().statement_list().statements(), r);
                if let Some(catch_) = try_stmt.catch() {
                    self.walk_statement_list(catch_.block().statement_list().statements(), r);
                }
                if let Some(finally_) = try_stmt.finally() {
                    self.walk_statement_list(finally_.block().statement_list().statements(), r);
                }
            }
            Statement::Return(ret) => {
                if let Some(expr) = ret.target() {
                    self.walk_expression(expr, r);
                }
            }
            Statement::Throw(th) => self.walk_expression(th.target(), r),
            Statement::Expression(expr) => self.walk_expression(expr, r),
            Statement::Labelled(l) => match l.item() {
                boa_ast::statement::LabelledItem::Statement(s) => self.walk_statement(s, r),
                boa_ast::statement::LabelledItem::FunctionDeclaration(_) => {
                    r.uses_functions = true;
                }
            },
            Statement::With(_) => {
                r.dangerous_imports.push("with-statement".to_string());
            }
            Statement::Var(var) => {
                for v in var.0.as_ref() {
                    if let Some(init) = v.init() {
                        self.walk_expression(init, r);
                    }
                }
            }
            Statement::Continue(_) | Statement::Break(_) | Statement::Empty => {}
        }
        r.estimated_complexity += 1;
    }

    fn walk_declaration(&self, decl: &Declaration, r: &mut FeatureReport) {
        match decl {
            Declaration::FunctionDeclaration(f) => {
                r.uses_functions = true;
                if f.contains_direct_eval() {
                    r.dangerous_imports.push("eval-like".to_string());
                }
            }
            Declaration::AsyncFunctionDeclaration(_af) => {
                r.uses_functions = true;
                r.uses_async = true;
            }
            Declaration::GeneratorDeclaration(_) | Declaration::AsyncGeneratorDeclaration(_) => {
                r.uses_functions = true;
            }
            Declaration::ClassDeclaration(_) => {
                r.uses_classes = true;
            }
            Declaration::Lexical(lex) => {
                let list = match lex {
                    LexicalDeclaration::Const(list) | LexicalDeclaration::Let(list) => list,
                };
                for v in list.as_ref() {
                    if let Some(init) = v.init() {
                        self.walk_expression(init, r);
                    }
                }
            }
        }
        r.estimated_complexity += 1;
    }

    fn walk_expression(&self, expr: &Expression, r: &mut FeatureReport) {
        match expr {
            Expression::Call(call) => {
                if let Expression::Identifier(id) = call.function() {
                    let name = self.interner.resolve_expect(id.sym());
                    if name.utf8() == Some("eval") || name.utf8() == Some("Function") {
                        r.dangerous_imports.push(format!("eval-like: {name}"));
                    }
                }
                for arg in call.args() {
                    self.walk_expression(arg, r);
                }
            }
            Expression::Await(_) => r.uses_async = true,
            Expression::Binary(b) => {
                self.walk_expression(b.lhs(), r);
                self.walk_expression(b.rhs(), r);
            }
            Expression::Unary(u) => self.walk_expression(u.target(), r),
            Expression::Update(u) => {
                match u.target() {
                    UpdateTarget::Identifier(_) => {}
                    UpdateTarget::PropertyAccess(pa) => {
                        self.walk_property_access(pa, r);
                    }
                }
            }
            Expression::Assign(a) => {
                match a.lhs() {
                    AssignTarget::Identifier(_) => {}
                    AssignTarget::Access(pa) => {
                        self.walk_property_access(pa, r);
                    }
                    AssignTarget::Pattern(_) => {}
                }
                self.walk_expression(a.rhs(), r);
            }
            Expression::Conditional(c) => {
                self.walk_expression(c.condition(), r);
                self.walk_expression(c.if_true(), r);
                self.walk_expression(c.if_false(), r);
            }
            Expression::Optional(o) => {
                self.walk_expression(o.target(), r);
            }
            Expression::ArrayLiteral(a) => {
                for elem in a.as_ref() {
                    if let Some(e) = elem {
                        self.walk_expression(e, r);
                    }
                }
            }
            Expression::ObjectLiteral(o) => {
                for prop in o.properties() {
                    match prop {
                        PropertyDefinition::Property(_name, expr) => {
                            self.walk_expression(expr, r);
                        }
                        PropertyDefinition::IdentifierReference(_) => {}
                        PropertyDefinition::SpreadObject(expr) => {
                            self.walk_expression(expr, r);
                        }
                        PropertyDefinition::MethodDefinition(_) => {
                            r.uses_functions = true;
                        }
                        PropertyDefinition::CoverInitializedName(_id, expr) => {
                            self.walk_expression(expr, r);
                        }
                    }
                }
            }
            Expression::TemplateLiteral(t) => {
                for elem in t.elements() {
                    if let boa_ast::expression::literal::TemplateElement::Expr(expr) = elem {
                        self.walk_expression(expr, r);
                    }
                }
            }
            Expression::TaggedTemplate(t) => {
                self.walk_expression(t.tag(), r);
                for arg in t.exprs() {
                    self.walk_expression(arg, r);
                }
            }
            Expression::FunctionExpression(fe) => {
                r.uses_functions = true;
                self.walk_statement_list(fe.body().statements(), r);
            }
            Expression::ArrowFunction(af) => {
                r.uses_functions = true;
                self.walk_statement_list(af.body().statements(), r);
            }
            Expression::New(n) => {
                self.walk_expression(n.constructor(), r);
                for arg in n.arguments() {
                    self.walk_expression(arg, r);
                }
            }
            Expression::Spread(s) => self.walk_expression(s.target(), r),
            Expression::Yield(y) => {
                if let Some(expr) = y.target() {
                    self.walk_expression(expr, r);
                }
            }
            Expression::PropertyAccess(pa) => self.walk_property_access(pa, r),
            Expression::RegExpLiteral(_)
            | Expression::NewTarget(_)
            | Expression::ImportMeta(_)
            | Expression::This(_)
            | Expression::ImportCall(_)
            | Expression::SuperCall(_)
            | Expression::Literal(_)
            | Expression::Identifier(_)
            | Expression::Parenthesized(_)
            | Expression::FormalParameterList(_)
            | Expression::Debugger
            | Expression::BinaryInPrivate(_)
            | Expression::AsyncArrowFunction(_)
            | Expression::GeneratorExpression(_)
            | Expression::AsyncFunctionExpression(_)
            | Expression::AsyncGeneratorExpression(_)
            | Expression::ClassExpression(_) => {}
        }
    }

    fn walk_property_access(&self, pa: &PropertyAccess, r: &mut FeatureReport) {
        match pa {
            PropertyAccess::Simple(spa) => {
                self.walk_expression(spa.target(), r);
            }
            PropertyAccess::Private(ppa) => {
                self.walk_expression(ppa.target(), r);
            }
            PropertyAccess::Super(_) => {}
        }
    }
}
