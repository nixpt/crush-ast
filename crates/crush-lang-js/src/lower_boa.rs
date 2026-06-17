use std::collections::HashMap;

use boa_ast::declaration::{Binding, Declaration, LexicalDeclaration, Variable};
use boa_ast::expression::access::{PropertyAccess, PropertyAccessField};
use boa_ast::expression::literal::{LiteralKind, PropertyDefinition, TemplateElement};
use boa_ast::expression::operator::assign::{AssignOp, AssignTarget};
use boa_ast::expression::operator::binary::BinaryOp;
use boa_ast::expression::operator::unary::UnaryOp;
use boa_ast::expression::operator::update::{UpdateOp, UpdateTarget};
use boa_ast::expression::{Expression, Identifier};
use boa_ast::function::{FormalParameterList, FunctionBody};
use boa_ast::property::PropertyName;
use boa_ast::statement::iteration::{ForLoopInitializer, IterableLoopInitializer};
use boa_ast::{ModuleItem, Statement, StatementListItem};
use boa_interner::{Interner, Sym};
use crush_cast::{CastType, Expression as CastExpr, Function, Program, Statement as CastStmt};

use crate::backend::boa::BoaAst;

fn meta() -> HashMap<String, serde_json::Value> {
    HashMap::new()
}

pub struct BoaLower {
    pub functions: HashMap<String, Function>,
    pub interner: Interner,
}

impl BoaLower {
    pub fn new(interner: Interner) -> Self {
        Self {
            functions: HashMap::new(),
            interner,
        }
    }

    fn sym_str(&self, sym: Sym) -> String {
        self.interner.resolve_expect(sym).to_string()
    }

    fn binding_name(&self, binding: &Binding) -> String {
        match binding {
            Binding::Identifier(id) => self.sym_str(id.sym()),
            _ => "_".to_string(),
        }
    }

    fn list(&mut self, items: &[StatementListItem]) -> Vec<CastStmt> {
        let mut out = Vec::new();
        for item in items {
            match item {
                StatementListItem::Statement(s) => out.extend(self.stmt(s)),
                StatementListItem::Declaration(d) => out.extend(self.decl(d)),
            }
        }
        out
    }

    fn module_list(&mut self, items: &[ModuleItem]) -> Vec<CastStmt> {
        let mut out = Vec::new();
        for item in items {
            match item {
                ModuleItem::StatementListItem(sli) => match sli {
                    StatementListItem::Statement(s) => out.extend(self.stmt(s)),
                    StatementListItem::Declaration(d) => out.extend(self.decl(d)),
                },
                ModuleItem::ImportDeclaration(_) => {
                    // import declarations are handled by the analyzer; skip in lowering
                }
                ModuleItem::ExportDeclaration(_e) => {
                    // ExportDeclaration type is not publicly accessible;
                    // storing most exports is out of scope for this pass.
                }
            }
        }
        out
    }

    fn param_list(&self, params: &FormalParameterList) -> Vec<(String, CastType)> {
        params
            .as_ref()
            .iter()
            .map(|p| (self.binding_name(p.variable().binding()), CastType::Any))
            .collect()
    }

    fn stmt(&mut self, stmt: &Statement) -> Vec<CastStmt> {
        match stmt {
            Statement::Block(b) => self.list(b.statement_list().statements()),
            Statement::Var(var) => {
                let mut out = Vec::new();
                for v in var.0.as_ref() {
                    out.extend(self.variable(v));
                }
                out
            }
            Statement::Empty => vec![],
            Statement::Expression(e) => {
                vec![CastStmt::ExprStmt {
                    expr: self.expr(e),
                    meta: meta(),
                }]
            }
            Statement::If(i) => {
                let condition = self.expr(i.cond());
                let then_body = self.stmt_wrap(i.body());
                let else_body = i.else_node().map(|a| self.stmt_wrap(a));
                vec![CastStmt::If {
                    condition,
                    then_body,
                    else_body,
                    meta: meta(),
                }]
            }
            Statement::WhileLoop(w) => {
                let condition = Box::new(self.expr(w.condition()));
                let body = self.stmt_wrap(w.body());
                vec![CastStmt::While {
                    condition,
                    body,
                    meta: meta(),
                }]
            }
            Statement::DoWhileLoop(dw) => {
                let condition = Box::new(self.expr(dw.cond()));
                let body = self.stmt_wrap(dw.body());
                vec![CastStmt::While {
                    condition,
                    body,
                    meta: meta(),
                }]
            }
            Statement::ForLoop(fl) => {
                let mut init_stmts = Vec::new();
                if let Some(init) = fl.init() {
                    match init {
                        ForLoopInitializer::Var(var) => {
                            for vd in var.0.as_ref() {
                                init_stmts.extend(self.variable(vd));
                            }
                        }
                        ForLoopInitializer::Lexical(lex) => {
                            for vd in lex.declaration().variable_list().as_ref() {
                                init_stmts.extend(self.variable(vd));
                            }
                        }
                        ForLoopInitializer::Expression(e) => {
                            init_stmts.push(CastStmt::ExprStmt {
                                expr: self.expr(e),
                                meta: meta(),
                            });
                        }
                    }
                };
                let test = fl
                    .condition()
                    .map(|c| self.expr(c))
                    .unwrap_or(CastExpr::BoolLiteral {
                        value: true,
                        meta: meta(),
                    });
                let body = self.stmt_wrap(fl.body());
                let mut while_body = vec![CastStmt::If {
                    condition: CastExpr::UnaryOp {
                        operator: "!".to_string(),
                        operand: Box::new(test),
                        meta: meta(),
                    },
                    then_body: vec![CastStmt::Break { meta: meta() }],
                    else_body: None,
                    meta: meta(),
                }];
                while_body.extend(body);
                init_stmts.push(CastStmt::While {
                    condition: Box::new(CastExpr::BoolLiteral {
                        value: true,
                        meta: meta(),
                    }),
                    body: while_body,
                    meta: meta(),
                });
                init_stmts
            }
            Statement::ForInLoop(fi) => {
                let variable = iter_loop_var(fi.initializer(), &self.interner);
                let iterable = Box::new(self.expr(fi.target()));
                let body = self.stmt_wrap(fi.body());
                vec![CastStmt::For {
                    variable,
                    iterable,
                    body,
                    meta: meta(),
                }]
            }
            Statement::ForOfLoop(fo) => {
                let variable = iter_loop_var(fo.initializer(), &self.interner);
                let iterable = Box::new(self.expr(fo.iterable()));
                let body = self.stmt_wrap(fo.body());
                vec![CastStmt::For {
                    variable,
                    iterable,
                    body,
                    meta: meta(),
                }]
            }
            Statement::Switch(s) => {
                let val = self.expr(s.val());
                let mut chain: Option<Vec<CastStmt>> = None;
                for case in s.cases() {
                    let cond = case.condition().map(|c| self.expr(c));
                    let body = self.list(case.body().statements());
                    if let Some(c) = cond {
                        let then_body = body;
                        let else_body = chain.take();
                        chain = Some(vec![CastStmt::If {
                            condition: CastExpr::BinaryOp {
                                operator: "===".to_string(),
                                left: Box::new(val.clone()),
                                right: Box::new(c),
                                meta: meta(),
                            },
                            then_body,
                            else_body,
                            meta: meta(),
                        }]);
                    } else {
                        // default case: attach as else branch of innermost if
                        if let Some(mut if_chain) = chain.take() {
                            if let Some(CastStmt::If { else_body, .. }) = if_chain.last_mut() {
                                *else_body = Some(body);
                            }
                            chain = Some(if_chain);
                        } else {
                            chain = Some(body);
                        }
                    }
                }
                chain.unwrap_or_default()
            }
            Statement::Continue(_) => vec![CastStmt::Continue { meta: meta() }],
            Statement::Break(_) => vec![CastStmt::Break { meta: meta() }],
            Statement::Return(r) => {
                let value = r.target().map(|e| self.expr(e));
                vec![CastStmt::Return {
                    value,
                    meta: meta(),
                }]
            }
            Statement::Labelled(l) => match l.item() {
                boa_ast::statement::LabelledItem::Statement(s) => self.stmt(s),
                boa_ast::statement::LabelledItem::FunctionDeclaration(f) => {
                    let name = self.sym_str(f.name().sym());
                    let func = self.make_fn(f.name(), f.parameters(), f.body());
                    self.functions.insert(name, func);
                    vec![]
                }
            },
            Statement::Throw(t) => {
                let value = self.expr(t.target());
                vec![CastStmt::Throw {
                    value,
                    meta: meta(),
                }]
            }
            Statement::Try(try_stmt) => {
                let body = self.list(try_stmt.block().statement_list().statements());
                let (error_var, handler) = try_stmt
                    .catch()
                    .map(|c| {
                        let ev = c
                            .parameter()
                            .map(|p| self.binding_name(p))
                            .unwrap_or_default();
                        let h = self.list(c.block().statement_list().statements());
                        (ev, h)
                    })
                    .unwrap_or_default();
                vec![CastStmt::TryCatch {
                    body,
                    error_var,
                    handler,
                    meta: meta(),
                }]
            }
            Statement::With(_) => vec![],
        }
    }

    fn stmt_wrap(&mut self, stmt: &Statement) -> Vec<CastStmt> {
        match stmt {
            Statement::Block(b) => self.list(b.statement_list().statements()),
            other => self.stmt(other),
        }
    }

    fn decl(&mut self, decl: &Declaration) -> Vec<CastStmt> {
        match decl {
            Declaration::FunctionDeclaration(f) => {
                let name = self.sym_str(f.name().sym());
                let func = self.make_fn(f.name(), f.parameters(), f.body());
                self.functions.insert(name.clone(), func);
                vec![CastStmt::FunctionDef {
                    name,
                    params: vec![],
                    body: vec![],
                    meta: meta(),
                }]
            }
            Declaration::AsyncFunctionDeclaration(af) => {
                let name = self.sym_str(af.name().sym());
                let func = self.make_fn(af.name(), af.parameters(), af.body());
                self.functions.insert(name.clone(), func);
                vec![CastStmt::FunctionDef {
                    name,
                    params: vec![],
                    body: vec![],
                    meta: meta(),
                }]
            }
            Declaration::GeneratorDeclaration(g) => {
                let name = self.sym_str(g.name().sym());
                let body = self.fn_body(g.body());
                let params = self.param_list(g.parameters());
                self.functions.insert(
                    name.clone(),
                    Function {
                        params,
                        body,
                        meta: HashMap::new(),
                    },
                );
                vec![CastStmt::FunctionDef {
                    name,
                    params: vec![],
                    body: vec![],
                    meta: meta(),
                }]
            }
            Declaration::AsyncGeneratorDeclaration(ag) => {
                let name = self.sym_str(ag.name().sym());
                let body = self.fn_body(ag.body());
                let params = self.param_list(ag.parameters());
                self.functions.insert(
                    name.clone(),
                    Function {
                        params,
                        body,
                        meta: HashMap::new(),
                    },
                );
                vec![CastStmt::FunctionDef {
                    name,
                    params: vec![],
                    body: vec![],
                    meta: meta(),
                }]
            }
            Declaration::ClassDeclaration(c) => {
                let name = self.sym_str(c.name().sym());
                vec![CastStmt::StructDef {
                    name,
                    fields: vec![],
                    meta: meta(),
                }]
            }
            Declaration::Lexical(lex) => {
                let mut out = Vec::new();
                match lex {
                    LexicalDeclaration::Const(list) | LexicalDeclaration::Let(list) => {
                        for v in list.as_ref() {
                            out.extend(self.variable(v));
                        }
                    }
                }
                out
            }
        }
    }

    fn variable(&mut self, var: &Variable) -> Vec<CastStmt> {
        let name = self.binding_name(var.binding());
        let value = var
            .init()
            .map(|e| self.expr(e))
            .unwrap_or(CastExpr::NullLiteral { meta: meta() });
        vec![CastStmt::VarDecl {
            name,
            value,
            type_hint: CastType::Any,
            meta: meta(),
        }]
    }

    fn make_fn(
        &mut self,
        _name: Identifier,
        params: &FormalParameterList,
        body: &FunctionBody,
    ) -> Function {
        let body_stmts = self.fn_body(body);
        let params = self.param_list(params);
        Function {
            params,
            body: body_stmts,
            meta: HashMap::new(),
        }
    }

    fn fn_body(&mut self, body: &FunctionBody) -> Vec<CastStmt> {
        self.list(body.statements())
    }

    fn call_name(&self, expr: &Expression) -> String {
        match expr {
            Expression::Identifier(id) => self.sym_str(id.sym()),
            Expression::PropertyAccess(pa) => self.prop_access_name(pa),
            _ => "<expr>".to_string(),
        }
    }

    fn prop_access_name(&self, pa: &PropertyAccess) -> String {
        match pa {
            PropertyAccess::Simple(spa) => {
                let target = self.call_name(spa.target());
                let field = match spa.field() {
                    PropertyAccessField::Const(id) => self.sym_str(id.sym()),
                    PropertyAccessField::Expr(_) => "[]".to_string(),
                };
                format!("{}.{}", target, field)
            }
            PropertyAccess::Private(r#priv) => {
                format!("#{}", self.sym_str(r#priv.field().description()))
            }
            PropertyAccess::Super(s) => {
                let field = match s.field() {
                    PropertyAccessField::Const(id) => self.sym_str(id.sym()),
                    PropertyAccessField::Expr(_) => "[]".to_string(),
                };
                format!("super.{}", field)
            }
        }
    }

    fn expr(&mut self, expr: &Expression) -> CastExpr {
        match expr {
            Expression::Literal(lit) => match lit.kind() {
                LiteralKind::String(sym) => CastExpr::StringLiteral {
                    value: self.sym_str(*sym),
                    meta: meta(),
                },
                LiteralKind::Num(n) => CastExpr::FloatLiteral {
                    value: *n,
                    meta: meta(),
                },
                LiteralKind::Int(n) => CastExpr::IntLiteral {
                    value: *n as i64,
                    meta: meta(),
                },
                LiteralKind::Bool(b) => CastExpr::BoolLiteral {
                    value: *b,
                    meta: meta(),
                },
                LiteralKind::Null | LiteralKind::Undefined => {
                    CastExpr::NullLiteral { meta: meta() }
                }
                LiteralKind::BigInt(_) => CastExpr::IntLiteral {
                    value: 0,
                    meta: meta(),
                },
            },
            Expression::Identifier(id) => CastExpr::Var {
                name: self.sym_str(id.sym()),
                meta: meta(),
            },
            Expression::Call(c) => {
                let name = self.call_name(c.function());
                let args: Vec<_> = c.args().iter().map(|a| self.expr(a)).collect();
                if name == "console.log" {
                    CastExpr::CapabilityCall {
                        name: "io.print".to_string(),
                        args: vec![CastExpr::CapabilityCall {
                            name: "io.format".to_string(),
                            args,
                            meta: meta(),
                        }],
                        meta: meta(),
                    }
                } else {
                    CastExpr::Call {
                        function: name,
                        args,
                        meta: meta(),
                    }
                }
            }
            Expression::New(n) => {
                let name = self.call_name(n.constructor());
                let args: Vec<_> = n.arguments().iter().map(|a| self.expr(a)).collect();
                CastExpr::Call {
                    function: format!("new_{}", name),
                    args,
                    meta: meta(),
                }
            }
            Expression::PropertyAccess(pa) => self.prop_access_expr(pa),
            Expression::Binary(b) => {
                let left = Box::new(self.expr(b.lhs()));
                let right = Box::new(self.expr(b.rhs()));
                let operator = binary_op_str(&b.op()).to_string();
                CastExpr::BinaryOp {
                    operator,
                    left,
                    right,
                    meta: meta(),
                }
            }
            Expression::Unary(u) => {
                let operator = unary_op_str(&u.op()).to_string();
                let operand = Box::new(self.expr(u.target()));
                CastExpr::UnaryOp {
                    operator,
                    operand,
                    meta: meta(),
                }
            }
            Expression::Update(u) => {
                let operator = update_op_str(&u.op()).to_string();
                let target = match u.target() {
                    UpdateTarget::Identifier(id) => CastExpr::Var {
                        name: self.sym_str(id.sym()),
                        meta: meta(),
                    },
                    UpdateTarget::PropertyAccess(pa) => self.prop_access_expr(pa),
                };
                CastExpr::UnaryOp {
                    operator,
                    operand: Box::new(target),
                    meta: meta(),
                }
            }
            Expression::Assign(a) => {
                let rhs = self.expr(a.rhs());
                let lhs_expr = match a.lhs() {
                    AssignTarget::Identifier(id) => CastExpr::Var {
                        name: self.sym_str(id.sym()),
                        meta: meta(),
                    },
                    AssignTarget::Access(pa) => self.prop_access_expr(pa),
                    _ => CastExpr::NullLiteral { meta: meta() },
                };
                let operator = if a.op() == AssignOp::Assign {
                    "=".to_string()
                } else {
                    format!("{}=", assign_op_str(&a.op()))
                };
                CastExpr::BinaryOp {
                    operator,
                    left: Box::new(lhs_expr),
                    right: Box::new(rhs),
                    meta: meta(),
                }
            }
            Expression::Conditional(c) => {
                let test = self.expr(c.condition());
                let cons = self.expr(c.if_true());
                let alt = self.expr(c.if_false());
                CastExpr::Call {
                    function: "__crush_ifexpr__".to_string(),
                    args: vec![test, cons, alt],
                    meta: meta(),
                }
            }
            Expression::This(_) => CastExpr::Var {
                name: "this".to_string(),
                meta: meta(),
            },
            Expression::ArrayLiteral(arr) => {
                let elements: Vec<_> = arr
                    .as_ref()
                    .iter()
                    .map(|e| {
                        e.as_ref()
                            .map(|inner| self.expr(inner))
                            .unwrap_or(CastExpr::NullLiteral { meta: meta() })
                    })
                    .collect();
                CastExpr::ArrayLiteral {
                    elements,
                    meta: meta(),
                }
            }
            Expression::ObjectLiteral(obj) => {
                let mut properties = Vec::new();
                for p in obj.properties() {
                    match p {
                        PropertyDefinition::Property(name, val) => {
                            let key = prop_name_str(name, &self.interner);
                            properties.push((key, self.expr(val)));
                        }
                        PropertyDefinition::IdentifierReference(id) => {
                            let k = self.sym_str(id.sym());
                            properties.push((
                                k.clone(),
                                CastExpr::Var {
                                    name: k,
                                    meta: meta(),
                                },
                            ));
                        }
                        PropertyDefinition::SpreadObject(expr) => {
                            properties.push(("__spread__".to_string(), self.expr(expr)));
                        }
                        PropertyDefinition::MethodDefinition(m) => {
                            let key = prop_name_str(m.name(), &self.interner);
                            let params: Vec<(String, CastType)> = m
                                .parameters()
                                .as_ref()
                                .iter()
                                .map(|p| (self.binding_name(p.variable().binding()), CastType::Any))
                                .collect();
                            let body = self.list(m.body().statements());
                            properties.push((
                                key,
                                CastExpr::Lambda {
                                    params,
                                    body,
                                    meta: meta(),
                                },
                            ));
                        }
                        PropertyDefinition::CoverInitializedName(id, expr) => {
                            let k = self.sym_str(id.sym());
                            properties.push((k, self.expr(expr)));
                        }
                    }
                }
                CastExpr::ObjectLiteral {
                    properties,
                    meta: meta(),
                }
            }
            Expression::TemplateLiteral(t) => {
                let mut parts = Vec::new();
                for elem in t.elements() {
                    match elem {
                        TemplateElement::String(sym) => {
                            parts.push(CastExpr::StringLiteral {
                                value: self.sym_str(*sym),
                                meta: meta(),
                            });
                        }
                        TemplateElement::Expr(e) => {
                            parts.push(self.expr(e));
                        }
                    }
                }
                if parts.is_empty() {
                    return CastExpr::StringLiteral {
                        value: String::new(),
                        meta: meta(),
                    };
                }
                let mut result = parts.remove(0);
                for part in parts {
                    result = CastExpr::BinaryOp {
                        operator: "+".to_string(),
                        left: Box::new(result),
                        right: Box::new(part),
                        meta: meta(),
                    };
                }
                result
            }
            Expression::TaggedTemplate(tt) => {
                let name = self.call_name(tt.tag());
                let args: Vec<_> = tt.exprs().iter().map(|a| self.expr(a)).collect();
                CastExpr::Call {
                    function: name,
                    args,
                    meta: meta(),
                }
            }
            Expression::FunctionExpression(fe) => {
                let name = fe.name().map(|n| self.sym_str(n.sym()));
                let body = self.fn_body(fe.body());
                let params = self.param_list(fe.parameters());
                if let Some(n) = name {
                    self.functions.insert(
                        n.clone(),
                        Function {
                            params: params.clone(),
                            body: body.clone(),
                            meta: HashMap::new(),
                        },
                    );
                    CastExpr::Var {
                        name: n,
                        meta: meta(),
                    }
                } else {
                    CastExpr::Lambda {
                        params,
                        body,
                        meta: meta(),
                    }
                }
            }
            Expression::ArrowFunction(af) => {
                let params = self.param_list(af.parameters());
                let body = self.fn_body(af.body());
                CastExpr::Lambda {
                    params,
                    body,
                    meta: meta(),
                }
            }
            Expression::ClassExpression(ce) => {
                let name = ce.name().map(|n| self.sym_str(n.sym())).unwrap_or_default();
                if name.is_empty() {
                    CastExpr::NullLiteral { meta: meta() }
                } else {
                    CastExpr::Var { name, meta: meta() }
                }
            }
            Expression::Await(a) => CastExpr::Await {
                expression: Box::new(self.expr(a.target())),
                meta: meta(),
            },
            Expression::Yield(y) => y
                .target()
                .map(|e| self.expr(e))
                .unwrap_or(CastExpr::NullLiteral { meta: meta() }),
            Expression::Spread(spread) => self.expr(spread.target()),
            Expression::Optional(opt) => self.expr(opt.target()),
            Expression::Parenthesized(p) => self.expr(p.expression()),
            Expression::RegExpLiteral(_)
            | Expression::SuperCall(_)
            | Expression::ImportCall(_)
            | Expression::NewTarget(_)
            | Expression::ImportMeta(_)
            | Expression::BinaryInPrivate(_)
            | Expression::AsyncArrowFunction(_)
            | Expression::GeneratorExpression(_)
            | Expression::AsyncFunctionExpression(_)
            | Expression::AsyncGeneratorExpression(_)
            | Expression::FormalParameterList(_)
            | Expression::Debugger => CastExpr::NullLiteral { meta: meta() },
        }
    }

    fn prop_access_expr(&mut self, pa: &PropertyAccess) -> CastExpr {
        match pa {
            PropertyAccess::Simple(spa) => {
                let target = Box::new(self.expr(spa.target()));
                match spa.field() {
                    PropertyAccessField::Const(id) => CastExpr::GetField {
                        target,
                        field: self.sym_str(id.sym()),
                        meta: meta(),
                    },
                    PropertyAccessField::Expr(e) => CastExpr::Index {
                        target,
                        index: Box::new(self.expr(e)),
                        meta: meta(),
                    },
                }
            }
            PropertyAccess::Private(r#priv) => {
                let target = Box::new(self.expr(r#priv.target()));
                CastExpr::GetField {
                    target,
                    field: self.sym_str(r#priv.field().description()),
                    meta: meta(),
                }
            }
            PropertyAccess::Super(s) => {
                let target = Box::new(CastExpr::Var {
                    name: "super".to_string(),
                    meta: meta(),
                });
                match s.field() {
                    PropertyAccessField::Const(id) => CastExpr::GetField {
                        target,
                        field: self.sym_str(id.sym()),
                        meta: meta(),
                    },
                    PropertyAccessField::Expr(e) => CastExpr::Index {
                        target,
                        index: Box::new(self.expr(e)),
                        meta: meta(),
                    },
                }
            }
        }
    }
}

fn iter_loop_var(init: &IterableLoopInitializer, interner: &Interner) -> String {
    match init {
        IterableLoopInitializer::Identifier(id) => interner.resolve_expect(id.sym()).to_string(),
        _ => "_".to_string(),
    }
}

fn prop_name_str(name: &PropertyName, interner: &Interner) -> String {
    match name {
        PropertyName::Literal(id) => interner.resolve_expect(id.sym()).to_string(),
        PropertyName::Computed(_) => "[]".to_string(),
    }
}

fn binary_op_str(op: &BinaryOp) -> &'static str {
    use boa_ast::expression::operator::binary::*;
    match op {
        BinaryOp::Arithmetic(ArithmeticOp::Add) => "+",
        BinaryOp::Arithmetic(ArithmeticOp::Sub) => "-",
        BinaryOp::Arithmetic(ArithmeticOp::Mul) => "*",
        BinaryOp::Arithmetic(ArithmeticOp::Div) => "/",
        BinaryOp::Arithmetic(ArithmeticOp::Mod) => "%",
        BinaryOp::Arithmetic(ArithmeticOp::Exp) => "**",
        BinaryOp::Bitwise(BitwiseOp::And) => "&",
        BinaryOp::Bitwise(BitwiseOp::Or) => "|",
        BinaryOp::Bitwise(BitwiseOp::Xor) => "^",
        BinaryOp::Bitwise(BitwiseOp::Shl) => "<<",
        BinaryOp::Bitwise(BitwiseOp::Shr) => ">>",
        BinaryOp::Bitwise(BitwiseOp::UShr) => ">>>",
        BinaryOp::Relational(RelationalOp::Equal) => "==",
        BinaryOp::Relational(RelationalOp::NotEqual) => "!=",
        BinaryOp::Relational(RelationalOp::StrictEqual) => "===",
        BinaryOp::Relational(RelationalOp::StrictNotEqual) => "!==",
        BinaryOp::Relational(RelationalOp::LessThan) => "<",
        BinaryOp::Relational(RelationalOp::LessThanOrEqual) => "<=",
        BinaryOp::Relational(RelationalOp::GreaterThan) => ">",
        BinaryOp::Relational(RelationalOp::GreaterThanOrEqual) => ">=",
        BinaryOp::Relational(RelationalOp::In) => "in",
        BinaryOp::Relational(RelationalOp::InstanceOf) => "instanceof",
        BinaryOp::Logical(LogicalOp::And) => "&&",
        BinaryOp::Logical(LogicalOp::Or) => "||",
        BinaryOp::Logical(LogicalOp::Coalesce) => "??",
        BinaryOp::Comma => ",",
    }
}

fn unary_op_str(op: &UnaryOp) -> &'static str {
    match op {
        UnaryOp::Minus => "-",
        UnaryOp::Plus => "+",
        UnaryOp::Not => "!",
        UnaryOp::Tilde => "~",
        UnaryOp::TypeOf => "typeof",
        UnaryOp::Void => "void",
        UnaryOp::Delete => "delete",
    }
}

fn update_op_str(op: &UpdateOp) -> &'static str {
    match op {
        UpdateOp::IncrementPost | UpdateOp::IncrementPre => "++",
        UpdateOp::DecrementPost | UpdateOp::DecrementPre => "--",
    }
}

fn assign_op_str(op: &AssignOp) -> &'static str {
    match op {
        AssignOp::Assign => "",
        AssignOp::Add => "+",
        AssignOp::Sub => "-",
        AssignOp::Mul => "*",
        AssignOp::Div => "/",
        AssignOp::Mod => "%",
        AssignOp::Exp => "**",
        AssignOp::And => "&",
        AssignOp::Or => "|",
        AssignOp::Xor => "^",
        AssignOp::Shl => "<<",
        AssignOp::Shr => ">>",
        AssignOp::Ushr => ">>>",
        AssignOp::BoolAnd => "&&",
        AssignOp::BoolOr => "||",
        AssignOp::Coalesce => "??",
    }
}

pub fn lower_boa(ast: BoaAst) -> anyhow::Result<Program> {
    let (mut lower, body) = match ast {
        BoaAst::Script(script, interner) => {
            let mut lower = BoaLower::new(interner);
            let body = lower.list(script.statements());
            (lower, body)
        }
        BoaAst::Module(module, interner) => {
            let mut lower = BoaLower::new(interner);
            let body = lower.module_list(module.items().items());
            (lower, body)
        }
    };
    lower.functions.insert(
        "main".to_string(),
        Function {
            params: vec![],
            body,
            meta: HashMap::new(),
        },
    );
    Ok(Program {
        cast_version: "0.2".to_string(),
        entry: "main".to_string(),
        lang: Some("javascript".to_string()),
        functions: lower.functions,
        ai_meta: None,
    })
}
