//! Expressions.

use boa_ast::expression::Expression;
use boa_ast::expression::access::{PropertyAccess, PropertyAccessField, SimplePropertyAccess};
use boa_ast::expression::{OptionalOperation, OptionalOperationKind};
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
use super::scope::{Param, Sig};
use super::stmt::{comma_parts, unparen};
use super::{Lower, MemberCall, Meta};
use crate::lower_common::{js_cap, js_div, length_of};

/// 2^63: the first integer an `i64` can't hold.
const I64_LIMIT: f64 = 9_223_372_036_854_775_808.0;

impl Lower<'_> {
    pub(super) fn expr(&mut self, e: &Expression) -> anyhow::Result<CastExpr> {
        // Track nesting: an assignment nested inside a larger
        // expression doesn't prove its kind (CRUSH-286 re-review).
        self.expr_depth += 1;
        let r = self.expr_top(e);
        self.expr_depth -= 1;
        r
    }

    fn expr_top(&mut self, e: &Expression) -> anyhow::Result<CastExpr> {
        let span = Some(e.span());
        let m = self.meta(span);
        match e {
            Expression::Identifier(id) => match self.user_binding(id.sym()) {
                // A function with default or rest parameters takes hidden
                // arguments its direct calls supply; called through a value,
                // it would get the wrong ones.
                Some(_) if self.binding_sig(id.sym()).is_some_and(|s| s.iter().any(|p| *p != Param::Plain)) => {
                    Err(self.unsupported(
                        format!(
                            "using `{}` as a value: a function with default or rest parameters can only be called directly",
                            self.name(id.sym())
                        ),
                        span,
                    ))
                }
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
            // CRUSH-277 D2: inside a class method, `this` is the receiver
            // parameter; anywhere else it stays loud (a nested function's
            // `this`, an arrow's lexical `this`, top-level `this`).
            Expression::This(_) => {
                if self.this_bound {
                    Ok(CastExpr::Var {
                        name: "this".to_string(),
                        meta: m,
                    })
                } else {
                    Err(self.unsupported("`this` outside a class method", span))
                }
            }
            Expression::New(n) => self.new_expr(n, span, m),
            Expression::SuperCall(sup) => {
                // `super(args)`: the parent's constructor on this
                // instance (CRUSH-277 S3). `super.m(...)` never arrives
                // here (a property access) and stays loud for S5.
                let Some(ctor) = self.super_ctor.clone() else {
                    return Err(self.unsupported(
                        "a `super(…)` call outside a derived class constructor",
                        span,
                    ));
                };
                // The parent constructor's parameters are known, so
                // default/rest parameters adapt (CRUSH-280).
                let adapted = match self.fn_sigs.get(&ctor).cloned() {
                    Some(sig) => self.adapt_args(&sig, sup.arguments(), "super", span)?,
                    None => self.args(sup.arguments())?,
                };
                Ok(CastExpr::Call {
                    function: ctor,
                    args: std::iter::once(CastExpr::Var {
                        name: "this".to_string(),
                        meta: m.clone(),
                    })
                    .chain(adapted)
                    .collect(),
                    meta: m,
                })
            }
            Expression::ClassExpression(c) => {
                let kind = self.class_expr(c)?;
                Ok(CastExpr::Var {
                    name: kind,
                    meta: m,
                })
            }
            Expression::Optional(o) => {
                self.optional_chain(o.target(), o.chain(), span, m)
            }
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

    pub(super) fn property_name(&mut self, key: &PropertyName, span: Option<Span>) -> anyhow::Result<String> {
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
        let (params, body) = self.function(params, body, span, false)?;
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
            "===" => return Ok(js_cap("js.strict_eq", vec![left, right], m)),
            "!==" => {
                return Ok(CastExpr::UnaryOp {
                    operator: "!".to_string(),
                    operand: Box::new(js_cap("js.strict_eq", vec![left, right], m.clone())),
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

    /// Optional chaining (CRUSH-289): boa flattens a chain into a
    /// target and operations, each shorted (`?.`) or not. A shorted
    /// operation becomes a null test over the chain so far — JS
    /// `== null`, matching both `null` and `undefined`, the `?.`
    /// trigger — with the rest of the chain lowered only on the
    /// non-null side, so a shorted call's arguments never evaluate.
    /// The prefix is evaluated twice (test and branch), so it must
    /// be pure; non-shorted operations fold into the prefix and
    /// lower through the ordinary access/call paths.
    fn optional_chain(
        &mut self,
        base: &Expression,
        ops: &[OptionalOperation],
        span: Option<Span>,
        m: Meta,
    ) -> anyhow::Result<CastExpr> {
        self.optional_chain2(base, base, ops, span, m)
    }

    /// `base` is the prefix the operations apply to (lowered as
    /// written, plus at most one type-erasing wrap — see below);
    /// `pure` is the same prefix unwrapped, which the purity check
    /// and the null test evaluate.
    fn optional_chain2(
        &mut self,
        base: &Expression,
        pure: &Expression,
        ops: &[OptionalOperation],
        span: Option<Span>,
        m: Meta,
    ) -> anyhow::Result<CastExpr> {
        let Some((first, rest)) = ops.split_first() else {
            return self.expr(base);
        };
        if !first.shorted() {
            let applied = self.optional_applied(first, base, span)?;
            let applied_pure = self.optional_applied(first, pure, span)?;
            return self.optional_chain2(&applied, &applied_pure, rest, span, m);
        }
        if has_side_effects(pure) {
            return Err(self.unsupported(
                "optional chaining through a side-effecting expression (the part before `?.` would be evaluated twice)",
                span,
            ));
        }
        // The test evaluates the lowered prefix (with its wrap);
        // `pure` is the same value unwrapped, for the check above.
        let cond = js_cap(
            "js.loose_eq",
            vec![
                self.expr(base)?,
                CastExpr::NullLiteral { meta: m.clone() },
            ],
            m.clone(),
        );
        // The branch applies the operation to the tested value. A
        // plain binding keeps the static type of its initializer in
        // the checker (`const n = null` is `Null`), which refuses a
        // field read even in this unreachable-when-null branch, so
        // such a binding is read back through `[n][0]` — the same
        // value, typed `Any`. Kind/host-tracked bindings and member
        // chains keep the plain receiver: their reads are `Any`
        // already, and their lowering is receiver-sensitive.
        let applied_base = match unparen(pure) {
            Expression::Identifier(id)
                if self.user_binding(id.sym()).is_some()
                    && !self.instance_kinds.contains_key(&self.resolve(id.sym()))
                    && !self.host_kinds.contains_key(&self.resolve(id.sym())) =>
            {
                let span_of = pure.span();
                Expression::PropertyAccess(PropertyAccess::Simple(
                    SimplePropertyAccess::new(
                        Expression::ArrayLiteral(boa_ast::expression::literal::ArrayLiteral::new(
                            vec![Some(pure.clone())],
                            false,
                            span_of,
                        )),
                        PropertyAccessField::Expr(Box::new(Expression::Literal(
                            boa_ast::expression::literal::Literal::new(0i32, span_of),
                        ))),
                    ),
                ))
            }
            _ => base.clone(),
        };
        let applied = self.optional_applied(first, &applied_base, span)?;
        let applied_pure = self.optional_applied(first, pure, span)?;
        let rest = self.optional_chain2(&applied, &applied_pure, rest, span, m.clone())?;
        Ok(CastExpr::Call {
            function: "__crush_ternary__".to_string(),
            args: vec![
                cond,
                CastExpr::NullLiteral { meta: m.clone() },
                rest,
            ],
            meta: m,
        })
    }

    /// One chain operation applied to a prefix, as a plain
    /// (non-optional) expression the ordinary lowering handles.
    fn optional_applied(
        &self,
        op: &OptionalOperation,
        base: &Expression,
        span: Option<Span>,
    ) -> anyhow::Result<Expression> {
        Ok(match op.kind() {
            OptionalOperationKind::SimplePropertyAccess { field } => {
                Expression::PropertyAccess(PropertyAccess::Simple(
                    SimplePropertyAccess::new(base.clone(), field.clone()),
                ))
            }
            OptionalOperationKind::PrivatePropertyAccess { .. } => {
                return Err(self.unsupported(
                    "a private field in an optional chain (`?.#x`)",
                    span,
                ));
            }
            OptionalOperationKind::Call { args } => {
                Expression::Call(boa_ast::expression::Call::new(
                    base.clone(),
                    args.clone(),
                    op.span(),
                ))
            }
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
                // A proven host collection (CRUSH-288): `.size` is
                // its length; any other member read — a method as a
                // value, a subscript into the pair array — refuses.
                if let Some(kind) = self.host_proof(s.target(), span) {
                    return match s.field() {
                        PropertyAccessField::Const(id)
                            if self.name(id.sym()) == "size" =>
                        {
                            let target = self.expr(s.target())?;
                            Ok(length_of(target, m))
                        }
                        PropertyAccessField::Const(id) => Err(self.unsupported(
                            format!(
                                "a {} has no property `.{}` (its methods are not values here)",
                                kind.label(),
                                self.name(id.sym())
                            ),
                            span,
                        )),
                        PropertyAccessField::Expr(_) => Err(self.unsupported(
                            "subscripting a Map/Set (use .get() and .set())",
                            span,
                        )),
                    };
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
            PropertyAccess::Super(_) => Err(self.unsupported(
                "a `super.x` read (only `super.m()` calls are lowered yet)",
                span,
            )),
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
                // Writing a member of a proven host collection
                // (`m[k] = v`, `m.x = v`) would write its array
                // representation: refused (CRUSH-288).
                if self.host_proof(s.target(), span).is_some() {
                    return Err(self.unsupported(
                        match s.field() {
                            PropertyAccessField::Expr(_) => {
                                "subscripting a Map/Set (use .get() and .set())"
                            }
                            _ => "assigning a property on a Map/Set",
                        },
                        span,
                    ));
                }
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
        // `c = new Counter()`: reassignment also makes the target a
        // known instance of a known kind (CRUSH-277 D4). Anything else
        // forgets it (CRUSH-286 review #3): the tracker must never
        // adapt a call for a class the binding no longer provably
        // holds. The re-review narrowed "anything else":
        // - only a plain `=` proves the new kind — a compound or
        //   logical operator (`c ||= new B()`) may keep the old value;
        // - only at the top of the expression being lowered — an
        //   assignment nested in a larger expression
        //   (`f && (c = new B())`, `f ? (c = new B()) : 0`) may never
        //   run, so it forgets instead;
        // - inside a nested function the assignment poisons the
        //   binding for good: a call to that function can swap it at
        //   any later point.
        if let Target::Var(name) = &target {
            if self.branch_depth > 0 {
                self.flow_assigned.insert(name.clone());
            }
            if self.fn_depth > 0 {
                self.fn_assigned.insert(name.clone());
            }
            let proves = a.op() == AssignOp::Assign && self.expr_depth <= 1;
            // A host collection proves (and forgets) by the same
            // rules; a binding is in at most one of the two maps
            // (CRUSH-288).
            if proves && let Some(kind) = self.new_kind(a.rhs()) {
                self.instance_kinds.insert(name.clone(), kind);
                self.host_kinds.remove(name);
            } else if proves && let Some(kind) = self.host_kind_of(a.rhs()) {
                self.host_kinds.insert(name.clone(), kind);
                self.instance_kinds.remove(name);
            } else {
                self.instance_kinds.remove(name);
                self.host_kinds.remove(name);
            }
        }
        // `this.f = …` in a method: a host-kind proof (or poison)
        // for the field, validated at end of module (CRUSH-288).
        if let AssignTarget::Access(PropertyAccess::Simple(s)) = a.lhs()
            && matches!(unparen(s.target()), Expression::This(_))
            && let PropertyAccessField::Const(field) = s.field()
        {
            let field = self.name(field.sym());
            self.note_field_assign(&field, a.op(), a.rhs());
        }
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
        // The write itself: `x = value` / `o[k] = value`, built the same way
        // for the plain and the logical path.
        let assign_to = |value: CastExpr| match &target {
            Target::Var(name) => CastExpr::Call {
                function: "__crush_assign__".to_string(),
                args: vec![
                    CastExpr::Var {
                        name: name.clone(),
                        meta: m.clone(),
                    },
                    value,
                ],
                meta: m.clone(),
            },
            Target::Slot(obj, key) => CastExpr::Call {
                function: "__crush_setindex__".to_string(),
                args: vec![obj.clone(), key.clone(), value],
                meta: m.clone(),
            },
        };
        let value = match a.op() {
            AssignOp::Assign => right,
            // `x &&= y` is `x && (x = y)`; `||=` and `??=` keep the current
            // value for a truthy / non-null `x` instead. The right side runs
            // only on the assign branch (`__crush_ternary__` jumps), and the
            // expression's value is the kept or newly assigned value.
            AssignOp::BoolAnd | AssignOp::BoolOr | AssignOp::Coalesce => {
                let kept = current(&target);
                let truthy = js_cap("js.truthy", vec![kept.clone()], m.clone());
                let keep = match a.op() {
                    AssignOp::BoolOr => truthy,
                    AssignOp::BoolAnd => CastExpr::UnaryOp {
                        operator: "!".to_string(),
                        operand: Box::new(truthy),
                        meta: m.clone(),
                    },
                    // `??`: null is the only nullish value here
                    // (`undefined` is a refused literal), so `!js.loose_eq(x,
                    // null)` is the `x ?? …` keep condition.
                    _ => CastExpr::UnaryOp {
                        operator: "!".to_string(),
                        operand: Box::new(js_cap(
                            "js.loose_eq",
                            vec![kept.clone(), CastExpr::NullLiteral { meta: m.clone() }],
                            m.clone(),
                        )),
                        meta: m.clone(),
                    },
                };
                // `__crush_assign__` leaves null (not the value), so the
                // assign branch writes first — as a `__crush_ternary__`
                // condition, whose value is dropped — and then re-reads the
                // target. Identical branches whatever that value is.
                let assigned = CastExpr::Call {
                    function: "__crush_ternary__".to_string(),
                    args: vec![assign_to(right), kept.clone(), kept.clone()],
                    meta: m.clone(),
                };
                // The conditional does the whole write; nothing wraps it.
                return Ok(CastExpr::Call {
                    function: "__crush_ternary__".to_string(),
                    args: vec![keep, kept, assigned],
                    meta: m.clone(),
                });
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
        Ok(assign_to(value))
    }

    /// Call arguments. A spread argument would lose its `...`.
    pub(super) fn args(&mut self, args: &[Expression]) -> anyhow::Result<Vec<CastExpr>> {
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
                // `C(...)` without `new` is not a call of the constructor
                // (CRUSH-277 D3): never silently construct. A binding of
                // the same spelling that shadows the class is called as
                // the value it is.
                if self.class_ref(id.sym()).is_some() {
                    return Err(self.unsupported(
                        format!(
                            "calling the class `{}` without `new`",
                            self.name(id.sym())
                        ),
                        span,
                    ));
                }
                if let Some(name) = self.user_binding(id.sym()) {
                    let args = match self.binding_sig(id.sym()) {
                        Some(sig) => self.adapt_args(&sig, args, &self.name(id.sym()), span)?,
                        None => self.args(args)?,
                    };
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
            Expression::PropertyAccess(PropertyAccess::Super(s)) => {
                let PropertyAccessField::Const(prop) = s.field() else {
                    return Err(self.unsupported("a computed `super[..]()` call", span));
                };
                let method = self.name(prop.sym());
                self.super_method_call(&method, args, span, m)
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
                // `C.f(...)` with `C` a class this module declares: a
                // static's target is known exactly (D3). From a
                // function body the class may only be pre-registered
                // so far (CRUSH-281).
                if let Expression::Identifier(obj) = unparen(s.target())
                    && let Some(kind) = self.class_ref(obj.sym())
                {
                    return match self.kind_static_fn(&kind, &method) {
                        // A static is called with its plain arguments; the
                        // VM's static member calls take no receiver either.
                        // Its default/rest parameters adapt (CRUSH-280).
                        Some(function) => {
                            let args = match self.fn_sigs.get(&function).cloned() {
                                Some(sig) => self.adapt_args(
                                    &sig,
                                    args,
                                    &format!("{}.{method}", self.name(obj.sym())),
                                    span,
                                )?,
                                None => self.args(args)?,
                            };
                            Ok(CastExpr::Call {
                                function,
                                args,
                                meta: m,
                            })
                        }
                        None => Err(self.unsupported(
                            format!(
                                "`{}.{method}(...)`: `{}` has no static method of that name (an instance method needs an instance and `new`)",
                                self.name(obj.sym()),
                                self.name(obj.sym()),
                            ),
                            span,
                        )),
                    };
                }
                // A proven host collection (`new Map()` / `new Set()`,
                // CRUSH-288): its operations are the synthesized
                // helpers, never the builtin table or the member-call
                // machinery, which would misread the representation.
                if let Some(kind) = self.host_proof(s.target(), span) {
                    let recv = self.expr(s.target())?;
                    return self.host_method_call(kind, recv, &method, args, span, m);
                }
                // `this.m(...)` in a method, or a variable this module
                // bound with `new C()` (D4): the dotted-`CapabilityCall`
                // shape the frontend compiles to a member call (the
                // leading dotted segment becomes the receiver, later
                // segments are read off the result). Any other receiver
                // falls through to the builtin table — conservative by
                // design, so builtin methods never flip to member calls.
                let dotted_recv = match unparen(s.target()) {
                    Expression::This(_) if self.this_bound => {
                        Some(("this".to_string(), self.home_class.clone()))
                    }
                    Expression::Identifier(obj) => {
                        // The RESOLVED name: a block-scoped binding is
                        // renamed, and the JS spelling would read an
                        // outer binding of the same name (CRUSH-285
                        // review #2) — or, with no outer binding, fall
                        // into a host `cap_call` for `name.method`. A
                        // binding some nested function assigns is
                        // never a known instance: calling that function
                        // can swap it at any point (CRUSH-286
                        // re-review).
                        let name = self.resolve(obj.sym());
                        if self.fn_assigned.contains(&name) {
                            None
                        } else {
                            self.instance_kinds
                                .get(&name)
                                .map(|kind| (name.clone(), Some(kind.clone())))
                        }
                    }
                    _ => None,
                };
                if let Some((recv, recv_kind)) = dotted_recv {
                    // The receiver's kind is known, so the method's
                    // default/rest parameters adapt (CRUSH-280); an
                    // unknown method keeps the raw arguments (the
                    // member-call machinery refuses it later, loudly).
                    // `this.m()` in a static method targets the class's
                    // static of that name: its function is known
                    // exactly (a static's `this` is the defining class),
                    // so it lowers to a direct call (CRUSH-286 review
                    // #4) — also an exact target, so it isn't recorded
                    // for the dispatch checks.
                    let is_static = self.in_static
                        && matches!(unparen(s.target()), Expression::This(_));
                    if is_static
                        && let Some(kind) = &recv_kind
                        && let Some((func, sig)) =
                            self.chain_target(kind, &method, true)
                    {
                        return Ok(CastExpr::Call {
                            function: func,
                            args: self.adapt_args(
                                &sig,
                                args,
                                &format!("this.{method}"),
                                span,
                            )?,
                            meta: m,
                        });
                    }
                    // In a static, `this.m()` shapes against the
                    // static signature, never the instance one
                    // (CRUSH-286 re-review, the `is_static` wiring):
                    // the exact-static path above already returned
                    // when one resolves, so this is the raw fallback.
                    let sig = if is_static {
                        recv_kind
                            .as_deref()
                            .and_then(|kind| self.chain_sig(kind, &method, true))
                    } else {
                        recv_kind
                            .as_deref()
                            .and_then(|kind| self.method_sig(kind, &method))
                    };
                    let args = match &sig {
                        Some(sig) => self.adapt_args(
                            sig,
                            args,
                            &format!("{recv}.{method}"),
                            span,
                        )?,
                        None => self.args(args)?,
                    };
                    // Recorded for the end-of-module checks: the
                    // shape must stay right under run-time dispatch
                    // (CRUSH-286 review #1/#2).
                    self.member_calls.push(MemberCall {
                        method: method.clone(),
                        kind: recv_kind.clone(),
                        adapted: sig,
                        is_static,
                        span,
                    });
                    // Marked as a walker-resolved member call: if the
                    // method turns out not to be a member, the compiler
                    // refuses — it must never fall back to granting a
                    // host capability named after the method
                    // (CRUSH-285 review #4).
                    let mut m = m;
                    m.insert("member_call".to_string(), serde_json::json!(true));
                    return Ok(CastExpr::CapabilityCall {
                        name: format!("{recv}.{method}"),
                        args,
                        meta: m,
                    });
                }
                // `new P(4).get2()`, `f().m(…)`: a method call on a
                // computed receiver resolves on its Kind at run time
                // (CRUSH-277 S5) — a value produced by `new`, a call or a
                // member read carries its own methods (a host value
                // errors loudly there instead). Literal receivers keep
                // the builtin rows (`[1, 2].push(…)`, `"a,b".split(…)`),
                // and bare identifiers stay with the conservative cases
                // above (D4).
                if !matches!(
                    unparen(s.target()),
                    Expression::Identifier(_)
                        | Expression::This(_)
                        | Expression::ArrayLiteral(_)
                        | Expression::ObjectLiteral(_)
                        | Expression::Literal(_)
                        | Expression::TemplateLiteral(_)
                ) {
                    // A member-access or call-result receiver
                    // (`this.items.push(x)`, `f().map(g)`): the builtin
                    // rows keep first claim, as they had before computed
                    // receivers lowered at all (CRUSH-285 review #7) —
                    // their refusals (the planned HOF rows) stay at walk
                    // time instead of surfacing as run-time member
                    // errors. A `new C()` result goes straight to the
                    // run-time member call: only a class instance
                    // carries Kind methods, and a builtin name (a Map's
                    // `get`) must not capture it.
                    if matches!(
                        unparen(s.target()),
                        Expression::PropertyAccess(_) | Expression::Call(_)
                    ) && let Some(row) = builtins::method(&method)
                    {
                        return self.builtin(row, Some(s.target()), args, statement, span, m);
                    }
                    // `new C().m(...)`: the receiver's kind is known,
                    // so the method's default/rest parameters adapt
                    // (CRUSH-280). Any other computed receiver keeps raw
                    // arguments — refused at the end of lowering if any
                    // member of the name has default/rest parameters
                    // they could land in (CRUSH-286 review #2).
                    let recv_kind = self.new_kind(s.target());
                    let sig = recv_kind
                        .as_deref()
                        .and_then(|kind| self.method_sig(kind, &method));
                    let args = match &sig {
                        Some(sig) => self.adapt_args(sig, args, &method, span)?,
                        None => self.args(args)?,
                    };
                    self.member_calls.push(MemberCall {
                        method: method.clone(),
                        kind: recv_kind,
                        adapted: sig,
                        is_static: false,
                        span,
                    });
                    return Ok(CastExpr::MethodCall {
                        target: Box::new(self.expr(s.target())?),
                        method,
                        args,
                        meta: m,
                    });
                }
                match builtins::method(&method) {
                    Some(row) => self.builtin(row, Some(s.target()), args, statement, span, m),
                    None => Err(self.unsupported(
                        format!("the method `.{method}()` (a method call on this value waits for CRUSH-277/zorro-44)"),
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

    /// The parameters of the `function` declaration `sym` names, if it names one.
    pub(super) fn binding_sig(&self, sym: boa_interner::Sym) -> Option<Sig> {
        if self.scopes.resolve(sym).is_some() {
            self.scopes.resolve_sig(sym)
        } else {
            self.toplevel_sigs.get(&sym).cloned()
        }
    }

    /// The arguments of a direct call to a `function` declaration, adapted to
    /// its parameters (CRUSH-zorro-18): a default parameter gets the argument
    /// or a placeholder plus a hidden "given" flag (appended after the
    /// parameters); a rest parameter gets the remaining arguments as an array.
    /// A missing plain argument is `undefined`, which crush doesn't have yet:
    /// refused. Extra arguments are evaluated and ignored in JS: dropped when
    /// that can't matter, refused otherwise.
    pub(super) fn adapt_args(
        &mut self,
        sig: &Sig,
        args: &[Expression],
        fname: &str,
        span: Option<Span>,
    ) -> anyhow::Result<Vec<CastExpr>> {
        if args.iter().any(|a| matches!(a, Expression::Spread(_))) {
            return self.spread_args(sig, args, fname, span);
        }
        let m = self.meta(span);
        let mut out = Vec::new();
        let mut flags = Vec::new();
        for (i, p) in sig.iter().enumerate() {
            match p {
                Param::Plain => match args.get(i) {
                    Some(a) => out.push(self.expr(a)?),
                    None => {
                        return Err(self.unsupported(
                            format!(
                                "calling `{fname}` without its argument {} (a missing argument is `undefined`, CRUSH-zorro-12)",
                                i + 1
                            ),
                            span,
                        ));
                    }
                },
                Param::Default => {
                    let given = args.get(i).is_some();
                    out.push(match args.get(i) {
                        Some(a) => self.expr(a)?,
                        None => CastExpr::NullLiteral { meta: m.clone() },
                    });
                    flags.push(CastExpr::BoolLiteral {
                        value: given,
                        meta: m.clone(),
                    });
                }
                Param::Rest => {
                    let rest = args.get(i..).unwrap_or(&[]);
                    let elements = rest
                        .iter()
                        .map(|a| self.expr(a))
                        .collect::<anyhow::Result<_>>()?;
                    out.push(CastExpr::ArrayLiteral {
                        elements,
                        meta: m.clone(),
                    });
                }
            }
        }
        if !sig.contains(&Param::Rest) {
            for extra in args.iter().skip(sig.len()) {
                if has_side_effects(extra) {
                    return Err(self.unsupported(
                        format!("calling `{fname}` with an extra argument that has side effects"),
                        Some(extra.span()),
                    ));
                }
            }
        }
        out.extend(flags);
        Ok(out)
    }

    /// The arguments of a direct call that includes a spread, adapted to a
    /// signature whose parameters are all plain (CRUSH-293 D2): the spread is
    /// expanded in place — `f(...xs)` becomes `f(xs[0], …, xs[N-1])` with N the
    /// parameter count — so the callee receives the values JS would pass it.
    ///
    /// A spread over an array literal is spliced in as its elements (D1), which
    /// needs no reads at all. A spread over a plain name is read by slot. A
    /// mixed list (`f(1, ...xs, 3)`, `f(...xs, ...ys)`) is modelled as one
    /// array — the ordinary arguments and each spread's elements, concatenated
    /// in order — and read by slot: the slot an element lands in depends on the
    /// lengths only at run time, so the concatenation is the argument list.
    /// Its pieces must be side-effect free, since each slot read evaluates it
    /// again.
    ///
    /// Semantics: a longer array is invisible (a fixed call drops the extras,
    /// exactly as JS ignores them); a shorter one is loud at run time (`array
    /// index out of range`), where JS would pass `undefined` (CRUSH-zorro-12).
    /// The source is assumed array-like — an object with a `length` is not
    /// iterated the way JS's spread is.
    fn spread_args(
        &mut self,
        sig: &Sig,
        args: &[Expression],
        fname: &str,
        span: Option<Span>,
    ) -> anyhow::Result<Vec<CastExpr>> {
        let m = self.meta(span);
        let spread_span = args
            .iter()
            .find_map(|a| match a {
                Expression::Spread(s) => Some(s.span()),
                _ => None,
            })
            .expect("a spread argument");
        // A default or rest parameter takes hidden arguments — a "given" flag
        // appended after the parameters, or one array slot — and a run-time
        // length can't produce those (CRUSH-zorro-18).
        if sig.iter().any(|p| *p != Param::Plain) {
            return Err(self.unsupported("a spread argument (`f(...args)`)", Some(spread_span)));
        }
        // The flattened argument list, in call order.
        enum Piece<'e> {
            /// An ordinary argument, or an element of a literal spread source.
            Known(&'e Expression),
            /// A spread over a plain name: expanded by reading it.
            Spread(&'e Expression),
        }
        let mut pieces: Vec<Piece> = Vec::new();
        for arg in args {
            let Expression::Spread(s) = arg else {
                pieces.push(Piece::Known(arg));
                continue;
            };
            match unparen(s.target()) {
                Expression::ArrayLiteral(lit) => {
                    for el in lit.as_ref() {
                        let Some(el) = el else {
                            return Err(
                                self.unsupported("an array hole (`[1, , 3]`)", Some(s.span()))
                            );
                        };
                        pieces.push(Piece::Known(el));
                    }
                }
                // A name reads the same array every time, so expanding it by
                // slot can't repeat an effect. Anything else would be
                // evaluated once per slot: refused.
                Expression::Identifier(_) => pieces.push(Piece::Spread(unparen(s.target()))),
                _ => {
                    return Err(self.unsupported(
                        "a spread argument (`f(...args)`) over something other than a plain name or an array literal",
                        Some(s.span()),
                    ));
                }
            }
        }
        if !pieces.iter().any(|p| matches!(p, Piece::Spread(_))) {
            // Every source was a literal: a positional list, which is all the
            // call needs. Truncating and short lists follow a plain call's
            // rules (extras dropped when that can't matter, a missing
            // argument refused).
            let mut out = Vec::new();
            for (i, p) in pieces.iter().enumerate() {
                let Piece::Known(e) = p else {
                    unreachable!("no spread piece")
                };
                if i >= sig.len() {
                    if has_side_effects(e) {
                        return Err(self.unsupported(
                            format!("calling `{fname}` with an extra argument that has side effects"),
                            Some(e.span()),
                        ));
                    }
                    continue;
                }
                out.push(self.expr(e)?);
            }
            if out.len() < sig.len() {
                return Err(self.unsupported(
                    format!(
                        "calling `{fname}` without its argument {} (a missing argument is `undefined`, CRUSH-zorro-12)",
                        out.len() + 1
                    ),
                    span,
                ));
            }
            return Ok(out);
        }
        // The model array: consecutive ordinary/literal arguments as one array
        // literal, each name spread concatenated in place (crush's `+`
        // concatenates arrays).
        let mut segments: Vec<CastExpr> = Vec::new();
        let mut run: Vec<CastExpr> = Vec::new();
        for p in &pieces {
            match p {
                Piece::Known(e) => {
                    if has_side_effects(e) {
                        return Err(self.unsupported(
                            "a spread argument (`f(...args)`) next to an argument with side effects",
                            Some(e.span()),
                        ));
                    }
                    run.push(self.expr(e)?);
                }
                Piece::Spread(e) => {
                    if !run.is_empty() {
                        segments.push(CastExpr::ArrayLiteral {
                            elements: std::mem::take(&mut run),
                            meta: m.clone(),
                        });
                    }
                    segments.push(self.expr(e)?);
                }
            }
        }
        if !run.is_empty() {
            segments.push(CastExpr::ArrayLiteral {
                elements: run,
                meta: m.clone(),
            });
        }
        let mut segments = segments.into_iter();
        let mut model = segments.next().expect("at least one segment");
        for segment in segments {
            model = CastExpr::BinaryOp {
                operator: "+".to_string(),
                left: Box::new(model),
                right: Box::new(segment),
                meta: m.clone(),
            };
        }
        Ok((0..sig.len() as i64)
            .map(|i| CastExpr::Index {
                target: Box::new(model.clone()),
                index: Box::new(CastExpr::IntLiteral {
                    value: i,
                    meta: m.clone(),
                }),
                meta: m.clone(),
            })
            .collect())
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
        // A spread is the real blocker, not the row's arity (`Math.max(...xs)`
        // counts as one argument): name it first (CRUSH-293).
        if let Some(Expression::Spread(s)) =
            args.iter().find(|a| matches!(a, Expression::Spread(_)))
        {
            return Err(self.unsupported("a spread argument (`f(...args)`)", Some(s.span())));
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
                let target = CastExpr::Var {
                    name: recv.clone(),
                    meta: m.clone(),
                };
                match args.len() {
                    // Nothing to append: the receiver (statement) or its length.
                    0 if statement => target,
                    0 => length_of(target, m),
                    1 => {
                        let call = CastExpr::Call {
                            function: format!("{recv}.push"),
                            args,
                            meta: m.clone(),
                        };
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
