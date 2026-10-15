//! Expressions.

use boa_ast::expression::Expression;
use boa_ast::expression::access::{PropertyAccess, PropertyAccessField};
use boa_ast::expression::literal::{LiteralKind, PropertyDefinition, TemplateElement};
use boa_ast::expression::operator::assign::{AssignOp, AssignTarget};
use boa_ast::expression::operator::binary::{
    ArithmeticOp, BinaryOp, BitwiseOp, LogicalOp, RelationalOp,
};
use boa_ast::expression::operator::unary::UnaryOp;
use boa_ast::expression::operator::update::{UpdateOp, UpdateTarget};
use boa_ast::function::{FormalParameterList, FunctionBody};
use boa_ast::property::{MethodDefinitionKind, PropertyName};
use boa_ast::{Span, Spanned};
use crush_cast::Expression as CastExpr;

use super::builtins::{self, Lowering};
use super::stmt::{comma_parts, unparen};
use super::{Lower, Meta};
use crate::lower_common::{js_cap, js_div, length_of};

/// 2^63: the first integer an `i64` can't hold.
const I64_LIMIT: f64 = 9_223_372_036_854_775_808.0;

impl Lower<'_> {
    pub(super) fn expr(&mut self, e: &Expression) -> anyhow::Result<CastExpr> {
        let span = Some(e.span());
        let m = self.meta(span);
        match e {
            Expression::Identifier(id) => match self.user_binding(id.sym()) {
                Some(name) => Ok(CastExpr::Var { name, meta: m }),
                None => Err(self.unbound(id.sym(), span)),
            },
            Expression::Literal(lit) => self.literal(lit.kind(), span, m),
            Expression::Parenthesized(p) => self.expr(p.expression()),
            Expression::Unary(u) => {
                let operator = match u.op() {
                    UnaryOp::Minus => "-",
                    UnaryOp::Plus => "+",
                    UnaryOp::Not => "!",
                    UnaryOp::Tilde => "~",
                    UnaryOp::TypeOf => "typeof",
                    UnaryOp::Void => "void",
                    UnaryOp::Delete => "delete",
                };
                let operand = self.expr(u.target())?;
                Ok(CastExpr::UnaryOp {
                    operator: operator.to_string(),
                    operand: Box::new(operand),
                    meta: m,
                })
            }
            Expression::Binary(b) => self.binary(b, span, m),
            Expression::Conditional(c) => {
                // `__crush_ternary__` compiles to jumps: only the taken branch runs.
                let args = vec![
                    self.expr(c.condition())?,
                    self.expr(c.if_true())?,
                    self.expr(c.if_false())?,
                ];
                Ok(CastExpr::Call {
                    function: "__crush_ternary__".to_string(),
                    args,
                    meta: m,
                })
            }
            Expression::Call(c) => self.call(c.function(), c.args(), false, span, m),
            Expression::PropertyAccess(pa) => self.access(pa, span, m),
            Expression::ArrayLiteral(a) => {
                let mut elements = Vec::new();
                for el in a.as_ref() {
                    match el {
                        Some(Expression::Spread(s)) => {
                            return Err(self.unsupported("array spread (`[...a]`)", Some(s.span())));
                        }
                        Some(e) => elements.push(self.expr(e)?),
                        // node prints a hole as `<1 empty item>`, not `null`.
                        None => return Err(self.unsupported("an array hole (`[1, , 3]`)", span)),
                    }
                }
                Ok(CastExpr::ArrayLiteral { elements, meta: m })
            }
            Expression::ObjectLiteral(o) => {
                let mut properties = Vec::new();
                for p in o.properties() {
                    match p {
                        PropertyDefinition::IdentifierReference(id) => {
                            let key = self.name(id.sym());
                            properties.push((
                                key,
                                CastExpr::Var {
                                    name: self.resolve(id.sym()),
                                    meta: m.clone(),
                                },
                            ));
                        }
                        PropertyDefinition::Property(key, value) => {
                            let key = self.property_name(key, span)?;
                            properties.push((key, self.expr(value)?));
                        }
                        PropertyDefinition::MethodDefinition(md) => {
                            if !matches!(md.kind(), MethodDefinitionKind::Ordinary) {
                                return Err(self.unsupported(
                                    "an object getter, setter, generator or async method",
                                    span,
                                ));
                            }
                            let key = self.property_name(md.name(), span)?;
                            properties.push((key, self.lambda(md.parameters(), md.body(), span)?));
                        }
                        PropertyDefinition::SpreadObject(_) => {
                            return Err(self.unsupported("object spread (`{...o}`)", span));
                        }
                        PropertyDefinition::CoverInitializedName(..) => {
                            return Err(self.unsupported("a shorthand default (`{a = 1}`)", span));
                        }
                    }
                }
                Ok(CastExpr::ObjectLiteral {
                    properties,
                    meta: m,
                })
            }
            Expression::ArrowFunction(f) => self.lambda(f.parameters(), f.body(), span),
            Expression::FunctionExpression(f) => self.lambda(f.parameters(), f.body(), span),
            Expression::Assign(a) => self.assign(a, span, m),
            Expression::Update(u) => {
                let UpdateTarget::Identifier(id) = u.target() else {
                    return Err(self.unsupported("`++`/`--` on a property or element", span));
                };
                let function = match u.op() {
                    UpdateOp::IncrementPre => "__crush_pre_inc__",
                    UpdateOp::DecrementPre => "__crush_pre_dec__",
                    UpdateOp::IncrementPost => "__crush_post_inc__",
                    UpdateOp::DecrementPost => "__crush_post_dec__",
                };
                let target = CastExpr::Var {
                    name: self.resolve(id.sym()),
                    meta: m.clone(),
                };
                Ok(CastExpr::Call {
                    function: function.to_string(),
                    args: vec![target],
                    meta: m,
                })
            }
            Expression::TemplateLiteral(t) => {
                // The cooked text, and each `${…}` through JS ToString
                // (`${1.5 * 4}` is `6`) via js.add onto the string so far.
                let mut result: Option<CastExpr> = None;
                for el in t.elements() {
                    let part = match el {
                        TemplateElement::String(s) => CastExpr::StringLiteral {
                            value: self.name(*s),
                            meta: m.clone(),
                        },
                        TemplateElement::Expr(e) => self.expr(e)?,
                    };
                    result = Some(match result {
                        None if matches!(el, TemplateElement::String(_)) => part,
                        None => js_cap(
                            "js.add",
                            vec![
                                CastExpr::StringLiteral {
                                    value: String::new(),
                                    meta: m.clone(),
                                },
                                part,
                            ],
                            m.clone(),
                        ),
                        Some(acc) => js_cap("js.add", vec![acc, part], m.clone()),
                    });
                }
                Ok(result.unwrap_or(CastExpr::StringLiteral {
                    value: String::new(),
                    meta: m,
                }))
            }
            Expression::Spread(_) => Err(self.unsupported("a spread (`...x`) here", span)),
            Expression::This(_) => {
                Err(self.unsupported("`this` (waits for the COSM object model)", span))
            }
            Expression::New(_) => {
                Err(self.unsupported("`new` (waits for the COSM object model)", span))
            }
            Expression::ClassExpression(_) => {
                Err(self.unsupported("a class (waits for the COSM object model)", span))
            }
            Expression::Optional(_) => Err(self.unsupported("optional chaining (`?.`)", span)),
            Expression::TaggedTemplate(_) => {
                Err(self.unsupported("a tagged template literal (tag`…`)", span))
            }
            Expression::RegExpLiteral(_) => {
                Err(self.unsupported("a regular expression literal", span))
            }
            Expression::AsyncArrowFunction(_) | Expression::AsyncFunctionExpression(_) => {
                Err(self.unsupported("an async function", span))
            }
            Expression::GeneratorExpression(_) => {
                Err(self.unsupported("a generator function (`function*`)", span))
            }
            Expression::AsyncGeneratorExpression(_) => {
                Err(self.unsupported("an async generator", span))
            }
            Expression::Await(_) => Err(self.unsupported("`await`", span)),
            Expression::Yield(_) => Err(self.unsupported("`yield`", span)),
            Expression::SuperCall(_) => Err(self.unsupported("`super(…)`", span)),
            Expression::ImportCall(_) => Err(self.unsupported("dynamic `import(…)`", span)),
            Expression::NewTarget(_) | Expression::ImportMeta(_) => {
                Err(self.unsupported("`import.meta` / `new.target`", span))
            }
            Expression::BinaryInPrivate(_) => Err(self.unsupported("`#x in obj`", span)),
        }
    }

    fn literal(
        &mut self,
        kind: &LiteralKind,
        span: Option<Span>,
        m: Meta,
    ) -> anyhow::Result<CastExpr> {
        Ok(match kind {
            LiteralKind::String(s) => CastExpr::StringLiteral {
                value: self.name(*s),
                meta: m,
            },
            LiteralKind::Int(i) => CastExpr::IntLiteral {
                value: i64::from(*i),
                meta: m,
            },
            // An integral value an `i64` holds stays an int (indexing, loops);
            // anything else, including integers past `i64`, is a float
            // (CRUSH-zorro-13, option (a)).
            LiteralKind::Num(n) if n.fract() == 0.0 && n.abs() < I64_LIMIT => {
                CastExpr::IntLiteral {
                    value: *n as i64,
                    meta: m,
                }
            }
            LiteralKind::Num(n) => CastExpr::FloatLiteral { value: *n, meta: m },
            LiteralKind::Bool(b) => CastExpr::BoolLiteral { value: *b, meta: m },
            LiteralKind::Null => CastExpr::NullLiteral { meta: m },
            LiteralKind::Undefined => return Err(self.unsupported("`undefined` as a value", span)),
            LiteralKind::BigInt(_) => return Err(self.unsupported("a BigInt literal (`1n`)", span)),
        })
    }

    fn property_name(&mut self, key: &PropertyName, span: Option<Span>) -> anyhow::Result<String> {
        match key {
            PropertyName::Literal(id) => Ok(self.name(id.sym())),
            PropertyName::Computed(Expression::Literal(l)) => match l.kind() {
                LiteralKind::String(s) => Ok(self.name(*s)),
                LiteralKind::Int(i) => Ok(i.to_string()),
                LiteralKind::Num(n) if n.fract() == 0.0 && n.abs() < I64_LIMIT => {
                    Ok((*n as i64).to_string())
                }
                _ => Err(self.unsupported("this property key", span)),
            },
            PropertyName::Computed(_) => {
                Err(self.unsupported("a computed property key (`[k]: v`)", span))
            }
        }
    }

    /// A function expression, arrow function or object method: a lambda,
    /// which sees the enclosing locals.
    pub(super) fn lambda(
        &mut self,
        params: &FormalParameterList,
        body: &FunctionBody,
        span: Option<Span>,
    ) -> anyhow::Result<CastExpr> {
        let (params, body) = self.function(params, body, span)?;
        Ok(CastExpr::Lambda {
            params,
            body,
            meta: self.meta(span),
        })
    }

    fn binary(
        &mut self,
        b: &boa_ast::expression::operator::Binary,
        span: Option<Span>,
        m: Meta,
    ) -> anyhow::Result<CastExpr> {
        if b.op() == BinaryOp::Comma {
            // `(a, b)` in value position: only the last value survives, so an
            // earlier part with side effects would be lost.
            let whole = Expression::Binary(b.clone());
            let mut parts = Vec::new();
            comma_parts(&whole, &mut parts);
            let (last, init) = parts.split_last().expect("two parts");
            if init.iter().any(|p| has_side_effects(p)) {
                return Err(self.unsupported(
                    "a comma expression with side effects in value position",
                    span,
                ));
            }
            return self.expr(last);
        }
        let numeric = is_js_number(b.lhs()) && is_js_number(b.rhs());
        let left = self.expr(b.lhs())?;
        let right = self.expr(b.rhs())?;
        let operator = match b.op() {
            BinaryOp::Arithmetic(op) => match op {
                ArithmeticOp::Add => "+",
                ArithmeticOp::Sub => "-",
                ArithmeticOp::Mul => "*",
                ArithmeticOp::Div => "/",
                ArithmeticOp::Mod => "%",
                ArithmeticOp::Exp => "**",
            },
            BinaryOp::Bitwise(op) => match op {
                BitwiseOp::And => "&",
                BitwiseOp::Or => "|",
                BitwiseOp::Xor => "^",
                BitwiseOp::Shl => "<<",
                BitwiseOp::Shr => ">>",
                BitwiseOp::UShr => ">>>",
            },
            BinaryOp::Relational(op) => match op {
                RelationalOp::Equal => "==",
                RelationalOp::NotEqual => "!=",
                RelationalOp::StrictEqual => "===",
                RelationalOp::StrictNotEqual => "!==",
                RelationalOp::GreaterThan => ">",
                RelationalOp::GreaterThanOrEqual => ">=",
                RelationalOp::LessThan => "<",
                RelationalOp::LessThanOrEqual => "<=",
                RelationalOp::In => "in",
                RelationalOp::InstanceOf => "instanceof",
            },
            BinaryOp::Logical(op) => match op {
                LogicalOp::And => "&&",
                LogicalOp::Or => "||",
                LogicalOp::Coalesce => "??",
            },
            BinaryOp::Comma => unreachable!("handled above"),
        };
        // JS semantics as `js.*` capabilities (crush-vm js_core); crush's own
        // operators are unchanged.
        match operator {
            "/" => return Ok(js_div(left, right, m)),
            "+" if !numeric => return Ok(js_cap("js.add", vec![left, right], m)),
            "==" => return Ok(js_cap("js.loose_eq", vec![left, right], m)),
            "!=" => {
                return Ok(CastExpr::UnaryOp {
                    operator: "!".to_string(),
                    operand: Box::new(js_cap("js.loose_eq", vec![left, right], m.clone())),
                    meta: m,
                });
            }
            // The deciding operand, not a bool (`0 || 'dflt'` is 'dflt'); the
            // right side runs only when the left doesn't decide.
            "||" | "&&" => {
                let function = if operator == "||" {
                    "__crush_js_or__"
                } else {
                    "__crush_js_and__"
                };
                return Ok(CastExpr::Call {
                    function: function.to_string(),
                    args: vec![left, right],
                    meta: m,
                });
            }
            _ => {}
        }
        Ok(CastExpr::BinaryOp {
            operator: operator.to_string(),
            left: Box::new(left),
            right: Box::new(right),
            meta: m,
        })
    }

    fn access(
        &mut self,
        pa: &PropertyAccess,
        span: Option<Span>,
        m: Meta,
    ) -> anyhow::Result<CastExpr> {
        match pa {
            PropertyAccess::Simple(s) => {
                // `Math.PI`: a namespace value, unless `Math` is a user binding.
                if let Expression::Identifier(obj) = unparen(s.target())
                    && self.user_binding(obj.sym()).is_none()
                    && let PropertyAccessField::Const(prop) = s.field()
                {
                    let name = format!("{}.{}", self.name(obj.sym()), self.name(prop.sym()));
                    return Err(match builtins::value(&name) {
                        Some(row) => self.planned(row, &format!("`{name}`"), span),
                        None => self.unbound(obj.sym(), span),
                    });
                }
                let target = self.expr(s.target())?;
                match s.field() {
                    // CRUSH-260: `x.length` is `len(x)`, except on an object literal.
                    PropertyAccessField::Const(id)
                        if self.name(id.sym()) == "length"
                            && !matches!(unparen(s.target()), Expression::ObjectLiteral(_)) =>
                    {
                        Ok(length_of(target, m))
                    }
                    PropertyAccessField::Const(id) => Ok(CastExpr::GetField {
                        target: Box::new(target),
                        field: self.name(id.sym()),
                        meta: m,
                    }),
                    PropertyAccessField::Expr(e) => {
                        let index = self.expr(e)?;
                        Ok(CastExpr::Index {
                            target: Box::new(target),
                            index: Box::new(index),
                            meta: m,
                        })
                    }
                }
            }
            PropertyAccess::Private(_) => Err(self.unsupported("a private field (`#x`)", span)),
            PropertyAccess::Super(_) => Err(self.unsupported("`super.x`", span)),
        }
    }

    fn assign(
        &mut self,
        a: &boa_ast::expression::operator::Assign,
        span: Option<Span>,
        m: Meta,
    ) -> anyhow::Result<CastExpr> {
        // Where the value goes: a variable, or an object field / array slot.
        enum Target {
            Var(String),
            Slot(CastExpr, CastExpr),
        }
        let target = match a.lhs() {
            AssignTarget::Identifier(id) => Target::Var(self.resolve(id.sym())),
            AssignTarget::Access(PropertyAccess::Simple(s)) => {
                let obj = self.expr(s.target())?;
                let key = match s.field() {
                    PropertyAccessField::Const(id) => CastExpr::StringLiteral {
                        value: self.name(id.sym()),
                        meta: m.clone(),
                    },
                    PropertyAccessField::Expr(e) => self.expr(e)?,
                };
                // A compound assignment reads the slot and writes it back, so
                // the object and key are evaluated twice: refuse that unless
                // they can't have side effects.
                if a.op() != AssignOp::Assign
                    && (has_side_effects(s.target())
                        || matches!(s.field(), PropertyAccessField::Expr(e) if has_side_effects(e)))
                {
                    return Err(self.unsupported(
                        "a compound assignment to a computed target with side effects",
                        span,
                    ));
                }
                Target::Slot(obj, key)
            }
            AssignTarget::Access(_) => {
                return Err(self.unsupported("assigning to a private or `super` field", span));
            }
            AssignTarget::Pattern(_) => {
                return Err(self.unsupported("destructuring assignment", span));
            }
        };
        let right = self.expr(a.rhs())?;
        let current = |t: &Target| match t {
            Target::Var(name) => CastExpr::Var {
                name: name.clone(),
                meta: m.clone(),
            },
            Target::Slot(obj, key) => CastExpr::Index {
                target: Box::new(obj.clone()),
                index: Box::new(key.clone()),
                meta: m.clone(),
            },
        };
        let binop = |op: &str, l: CastExpr, r: CastExpr| CastExpr::BinaryOp {
            operator: op.to_string(),
            left: Box::new(l),
            right: Box::new(r),
            meta: m.clone(),
        };
        let value = match a.op() {
            AssignOp::Assign => right,
            AssignOp::BoolAnd | AssignOp::BoolOr | AssignOp::Coalesce => {
                return Err(self.unsupported("logical assignment (`&&=`, `||=`, `??=`)", span));
            }
            AssignOp::Div => js_div(current(&target), right, m.clone()),
            AssignOp::Add => js_cap("js.add", vec![current(&target), right], m.clone()),
            AssignOp::Sub => binop("-", current(&target), right),
            AssignOp::Mul => binop("*", current(&target), right),
            AssignOp::Mod => binop("%", current(&target), right),
            AssignOp::Exp => binop("**", current(&target), right),
            AssignOp::And => binop("&", current(&target), right),
            AssignOp::Or => binop("|", current(&target), right),
            AssignOp::Xor => binop("^", current(&target), right),
            AssignOp::Shl => binop("<<", current(&target), right),
            AssignOp::Shr => binop(">>", current(&target), right),
            AssignOp::Ushr => binop(">>>", current(&target), right),
        };
        Ok(match target {
            Target::Var(name) => CastExpr::Call {
                function: "__crush_assign__".to_string(),
                args: vec![
                    CastExpr::Var {
                        name,
                        meta: m.clone(),
                    },
                    value,
                ],
                meta: m,
            },
            Target::Slot(obj, key) => CastExpr::Call {
                function: "__crush_setindex__".to_string(),
                args: vec![obj, key, value],
                meta: m,
            },
        })
    }

    /// Call arguments. A spread argument would lose its `...`.
    fn args(&mut self, args: &[Expression]) -> anyhow::Result<Vec<CastExpr>> {
        args.iter()
            .map(|a| match a {
                Expression::Spread(s) => {
                    Err(self.unsupported("a spread argument (`f(...args)`)", Some(s.span())))
                }
                a => self.expr(a),
            })
            .collect()
    }

    /// A call. A callee that is a user binding (a local, a parameter, a
    /// top-level or lifted function) is called by its crush name; anything
    /// else must be a row of the builtin/method table ([`super::builtins`]).
    /// `statement`: the call's value is discarded.
    pub(super) fn call(
        &mut self,
        callee: &Expression,
        args: &[Expression],
        statement: bool,
        span: Option<Span>,
        m: Meta,
    ) -> anyhow::Result<CastExpr> {
        match unparen(callee) {
            Expression::Identifier(id) => {
                if let Some(name) = self.user_binding(id.sym()) {
                    let args = self.args(args)?;
                    return Ok(CastExpr::Call {
                        function: name,
                        args,
                        meta: m,
                    });
                }
                let name = self.name(id.sym());
                match builtins::global(&name) {
                    Some(row) => self.builtin(row, None, args, statement, span, m),
                    None => {
                        Err(self
                            .unsupported(format!("calling `{name}`, which is not defined"), span))
                    }
                }
            }
            Expression::PropertyAccess(PropertyAccess::Simple(s)) => {
                let PropertyAccessField::Const(prop) = s.field() else {
                    return Err(self.unsupported("a computed method call (`o[k]()`)", span));
                };
                let method = self.name(prop.sym());
                // `Math.floor(x)`: a namespace function, unless `Math` is a user binding.
                if let Expression::Identifier(obj) = unparen(s.target())
                    && self.user_binding(obj.sym()).is_none()
                {
                    let name = format!("{}.{method}", self.name(obj.sym()));
                    return match builtins::global(&name) {
                        Some(row) => self.builtin(row, None, args, statement, span, m),
                        None => Err(self.unsupported(format!("`{name}()`"), span)),
                    };
                }
                match builtins::method(&method) {
                    Some(row) => self.builtin(row, Some(s.target()), args, statement, span, m),
                    None => Err(self.unsupported(
                        format!("the method `.{method}()` (a user object's method waits for the COSM object model)"),
                        span,
                    )),
                }
            }
            _ => Err(self.unsupported(
                "calling an expression that isn't a plain name (`(function () {…})()`, `f()()`)",
                span,
            )),
        }
    }

    /// The refusal for reading a name no scope binds.
    fn unbound(&self, sym: boa_interner::Sym, span: Option<Span>) -> anyhow::Error {
        let name = self.name(sym);
        if let Some(row) = builtins::value(&name) {
            return self.planned(row, &format!("`{name}` as a value"), span);
        }
        self.unsupported(format!("`{name}`, which is not defined"), span)
    }

    fn planned(&self, row: &builtins::Row, shown: &str, span: Option<Span>) -> anyhow::Error {
        match row.lower {
            Lowering::Planned(ticket) => self.unsupported(format!("{shown} ({ticket})"), span),
            _ => self.unsupported(shown.to_string(), span),
        }
    }

    /// The crush name a callee refers to, if it is a user binding.
    pub(super) fn user_binding(&self, sym: boa_interner::Sym) -> Option<String> {
        if self.scopes.resolve(sym).is_some() || self.toplevel_fns.contains(&sym) {
            Some(self.resolve(sym))
        } else {
            None
        }
    }

    /// A call to a table row; `receiver` for a method.
    fn builtin(
        &mut self,
        row: &builtins::Row,
        receiver: Option<&Expression>,
        args: &[Expression],
        statement: bool,
        span: Option<Span>,
        m: Meta,
    ) -> anyhow::Result<CastExpr> {
        let shown = match receiver {
            Some(_) => format!(".{}()", row.name),
            None => format!("{}()", row.name),
        };
        if let Lowering::Planned(ticket) = row.lower {
            return Err(self.unsupported(format!("`{shown}` ({ticket})"), span));
        }
        if !row.arity.accepts(args.len()) {
            return Err(
                self.unsupported(format!("`{shown}` with {} argument(s)", args.len()), span)
            );
        }
        let args = self.args(args)?;
        Ok(match row.lower {
            Lowering::Cap(name) => CastExpr::CapabilityCall {
                name: name.to_string(),
                args,
                meta: m,
            },
            // The line node prints: js.format (node's util.format).
            Lowering::ConsoleLine => CastExpr::CapabilityCall {
                name: "io.print".to_string(),
                args: vec![js_cap("js.format", args, m.clone())],
                meta: m,
            },
            Lowering::Receiver { cap, value } => {
                if !value && !statement {
                    return Err(self.unsupported(format!("using the result of `{shown}`"), span));
                }
                // The compiler dispatches `recv.m(…)` on a variable receiver.
                let Some(Expression::Identifier(recv)) = receiver.map(unparen) else {
                    return Err(self.unsupported(
                        "calling a method on anything but a plain name (`a.b.m()`, `\"s\".m()`, `f().m()`)",
                        span,
                    ));
                };
                let recv = self.resolve(recv.sym());
                CastExpr::Call {
                    function: format!("{recv}.{cap}"),
                    args,
                    meta: m,
                }
            }
            Lowering::Push => {
                let Some(Expression::Identifier(recv)) = receiver.map(unparen) else {
                    return Err(self.unsupported(
                        "calling a method on anything but a plain name (`a.b.m()`, `\"s\".m()`, `f().m()`)",
                        span,
                    ));
                };
                let recv = self.resolve(recv.sym());
                let target = CastExpr::Var { name: recv.clone(), meta: m.clone() };
                match args.len() {
                    // Nothing to append: the receiver (statement) or its length.
                    0 if statement => target,
                    0 => length_of(target, m),
                    1 => {
                        let call = CastExpr::Call { function: format!("{recv}.push"), args, meta: m.clone() };
                        if statement { call } else { length_of(call, m) }
                    }
                    // Statement position is expanded by `push_statements`.
                    n => {
                        return Err(self.unsupported(
                            format!("using the result of `{shown}` with {n} arguments"),
                            span,
                        ));
                    }
                }
            }
            Lowering::Planned(_) => unreachable!("refused above"),
        })
    }
}

/// Whether JS evaluates `e` to a number whatever the variables hold: number
/// literals and arithmetic on them. Such a `+` stays crush's numeric `+`; any
/// other `+` may involve a string and goes through `js.add`.
fn is_js_number(e: &Expression) -> bool {
    match unparen(e) {
        Expression::Literal(l) => matches!(l.kind(), LiteralKind::Int(_) | LiteralKind::Num(_)),
        Expression::Unary(u) => u.op() == UnaryOp::Minus && is_js_number(u.target()),
        Expression::Binary(b) => {
            matches!(b.op(), BinaryOp::Arithmetic(_))
                && is_js_number(b.lhs())
                && is_js_number(b.rhs())
        }
        _ => false,
    }
}

/// Could evaluating `e` change anything? Conservative: only names, literals
/// and operators over them are side-effect free.
fn has_side_effects(e: &Expression) -> bool {
    match unparen(e) {
        Expression::Identifier(_) | Expression::Literal(_) | Expression::This(_) => false,
        Expression::Unary(u) => u.op() == UnaryOp::Delete || has_side_effects(u.target()),
        Expression::Binary(b) => has_side_effects(b.lhs()) || has_side_effects(b.rhs()),
        Expression::Conditional(c) => {
            has_side_effects(c.condition())
                || has_side_effects(c.if_true())
                || has_side_effects(c.if_false())
        }
        Expression::PropertyAccess(PropertyAccess::Simple(s)) => {
            has_side_effects(s.target())
                || matches!(s.field(), PropertyAccessField::Expr(e) if has_side_effects(e))
        }
        Expression::TemplateLiteral(t) => t
            .elements()
            .iter()
            .any(|el| matches!(el, TemplateElement::Expr(e) if has_side_effects(e))),
        _ => true,
    }
}
