//! JS classes → COSM (CRUSH-277 S1): the first source-language walker that
//! emits COSM declarations.
//!
//! A class becomes a [`crush_cosm::KindDecl`] in `Program::cosm` — JS's row
//! of the strategy catalogue (`docs/design/object-model.md`): open-dict
//! storage, reference semantics, prototype-chain resolution, dynamic-`this`
//! receiver, `js_new` construction, identity equality. Its constructor,
//! methods, accessors and static methods become ordinary CAST functions in
//! `Program::functions` under lifted names (`Counter::increment`,
//! `Counter::constructor`, `Counter::get::count`, …), each taking the
//! receiver as a first parameter named `this` ([`Lower::method_function`]).
//!
//! With S2 (the VM runs prototype chains) and S3, this is also where
//! `new C(…)` becomes a CAST `Construct`, `super(...)` becomes a call of
//! the parent's constructor on the same instance, and bindings this
//! module bound with `new C()` become known instances whose method
//! calls lower to member calls. S5 adds the rest of the member shapes:
//! `super.m()` resolves from the home class's parent up to a direct
//! call, instance fields initialise on the instance at construction
//! (D7), static fields with constant initialisers are constants of the
//! kind, and method calls on computed receivers (`new P(4).get2()`)
//! lower to a CAST `MethodCall`. Everything outside the slice stays a
//! typed refusal here: private members, static blocks, computed member
//! names, async/generator methods, anonymous class expressions,
//! expression superclasses, non-constant static field initialisers,
//! `super.x` reads, and a derived class whose parent's constructor the
//! walker hasn't lowered (imported classes — CRUSH-110).

use boa_ast::expression::literal::LiteralKind;
use boa_ast::expression::operator::unary::UnaryOp;
use boa_ast::expression::{Expression, New};
use boa_ast::function::{
    ClassDeclaration, ClassElement, ClassElementName, ClassExpression, ClassMethodDefinition,
    FunctionExpression,
};
use boa_ast::property::{MethodDefinitionKind, PropertyName};
use boa_ast::{Span, Spanned};
use boa_interner::Sym;
use crush_cast::{CastType, Expression as CastExpr, Function, Statement as CastStmt};
use crush_cosm::{
    Construct, Equality, FnRef, Hooks, KindDecl, KindRef, Member, MemberDecl, Receiver, Resolution,
    Semantics, Storage,
};

use super::stmt::unparen;
use super::{Lower, Meta};

impl Lower<'_> {
    /// A class declaration: record its KindDecl; the methods are already
    /// lifted into `Program::functions` by the time this returns, so the
    /// declaration itself contributes no statements.
    pub(super) fn class_decl(&mut self, c: &ClassDeclaration) -> anyhow::Result<Vec<CastStmt>> {
        self.class_kind(
            Some(c.name().sym()),
            c.super_ref(),
            c.constructor(),
            c.elements(),
            Some(c.name().span()),
        )?;
        Ok(vec![])
    }

    /// A class expression. Boa supplies a name for the common shapes —
    /// the class's own (`class K { … }`) or the binding it infers
    /// (`const K = class { … }`) — and that name keys the KindDecl (and,
    /// from S3, the value the expression loads). A truly anonymous class
    /// (`f(class { … })`) has neither and stays loud. Returns the Kind's
    /// name, which is what the expression evaluates to.
    pub(super) fn class_expr(&mut self, c: &ClassExpression) -> anyhow::Result<String> {
        let Some(name) = c.name() else {
            return Err(self.unsupported(
                "an anonymous class expression (name it: `const K = class K { … }`)",
                None,
            ));
        };
        self.class_kind(
            Some(name.sym()),
            c.super_ref(),
            c.constructor(),
            c.elements(),
            Some(name.span()),
        )
    }

    /// Is `resolved` (a binding's crush name) a class this module has
    /// declared as a COSM kind?
    pub(super) fn is_kind(&self, resolved: &str) -> bool {
        self.kinds.iter().any(|k| k.name == resolved)
    }

    /// Is `e` a `new C()` of a class this module declares?
    pub(super) fn is_new_of_kind(&self, e: &Expression) -> bool {
        match unparen(e) {
            Expression::New(n) => match unparen(n.constructor()) {
                Expression::Identifier(id) => self.is_kind(&self.resolve(id.sym())),
                _ => false,
            },
            _ => false,
        }
    }

    /// `new C(args)` (CRUSH-277 D3): a CAST `Construct` of the Kind. A
    /// member or computed callee (`new a.B()`) and a name that isn't a
    /// class declared in this module (`new Set()`, `new THREE.X()`) stay
    /// loud — those construct host objects, not COSM kinds.
    pub(super) fn new_expr(
        &mut self,
        n: &New,
        span: Option<Span>,
        m: Meta,
    ) -> anyhow::Result<CastExpr> {
        let Expression::Identifier(id) = unparen(n.constructor()) else {
            return Err(self.unsupported(
                "`new` of anything but a class name (`new a.B()`, `new f()()`)",
                span,
            ));
        };
        let kind = self.resolve(id.sym());
        if !self.is_kind(&kind) {
            return Err(self.unsupported(
                format!(
                    "`new {}`: only a class declared in this module constructs yet",
                    self.name(id.sym())
                ),
                span,
            ));
        }
        Ok(CastExpr::Construct {
            kind,
            args: self.args(n.arguments())?,
            meta: m,
        })
    }

    /// `super.m(args)` (CRUSH-277 S5): JS starts member lookup at the
    /// parent of the class whose body textually contains the call (its
    /// [[HomeObject]]), with the same receiver — so the target is known
    /// exactly at walk time and lowers to a direct call of the ancestor's
    /// lifted method. Ancestor accessors and the static/instance divide
    /// stay loud.
    pub(super) fn super_method_call(
        &mut self,
        method: &str,
        args: &[Expression],
        span: Option<Span>,
        m: Meta,
    ) -> anyhow::Result<CastExpr> {
        let Some(home) = self.home_class.clone() else {
            return Err(self.unsupported(
                "a `super.m()` call outside a class method",
                span,
            ));
        };
        let mut cur = self.class_parents.get(&home).cloned();
        let mut seen = std::collections::HashSet::new();
        while let Some(ancestor) = cur {
            if !seen.insert(ancestor.clone()) {
                break;
            }
            if self.in_static {
                if let Some(f) = self.kind_static_fn(&ancestor, method) {
                    return Ok(CastExpr::Call {
                        function: f,
                        args: self.args(args)?,
                        meta: m,
                    });
                }
                if self.functions.contains_key(&format!("{ancestor}::{method}")) {
                    return Err(self.unsupported(
                        format!(
                            "`super.{method}()` where `{method}` is an instance method of `{ancestor}`, called from a static method"
                        ),
                        span,
                    ));
                }
            } else {
                if self.functions.contains_key(&format!("{ancestor}::{method}")) {
                    let args = std::iter::once(CastExpr::Var {
                        name: "this".to_string(),
                        meta: m.clone(),
                    })
                    .chain(self.args(args)?)
                    .collect();
                    return Ok(CastExpr::Call {
                        function: format!("{ancestor}::{method}"),
                        args,
                        meta: m,
                    });
                }
                if self.kind_static_fn(&ancestor, method).is_some() {
                    return Err(self.unsupported(
                        format!(
                            "`super.{method}()` where `{method}` is a static method of `{ancestor}`"
                        ),
                        span,
                    ));
                }
            }
            if self
                .kinds
                .iter()
                .find(|k| k.name == ancestor)
                .is_some_and(|k| {
                    k.members
                        .iter()
                        .any(|mm| mm.name == method && matches!(mm.member, Member::Accessor { .. }))
                })
            {
                return Err(self.unsupported(
                    format!(
                        "`super.{method}()` where `{method}` is an accessor of `{ancestor}`"
                    ),
                    span,
                ));
            }
            cur = self.class_parents.get(&ancestor).cloned();
        }
        Err(self.unsupported(
            format!(
                "`super.{method}()`: no ancestor of `{home}` defines `{method}` in this module"
            ),
            span,
        ))
    }

    /// A class field's name (computed names stay loud).
    fn field_name(
        &self,
        name: &PropertyName,
        span: Option<Span>,
    ) -> anyhow::Result<String> {
        match name {
            PropertyName::Literal(id) => Ok(self.name(id.sym())),
            _ => Err(self.unsupported("a computed member name (`[k]() {}`)", span)),
        }
    }

    /// A field initialiser's value (CRUSH-277 S5): JS evaluates it in
    /// the class scope — not in the constructor's parameter scope — with
    /// `this` bound to the instance being built.
    fn field_init(
        &mut self,
        init: Option<&Expression>,
        span: Option<Span>,
    ) -> anyhow::Result<CastExpr> {
        self.scopes.enter_fn();
        let saved_this = std::mem::replace(&mut self.this_bound, true);
        let saved_super = self.super_ctor.take();
        let saved_instances = std::mem::take(&mut self.instance_vars);
        let r = match init {
            Some(e) => self.expr(e),
            None => Ok(CastExpr::NullLiteral {
                meta: self.meta(span),
            }),
        };
        self.this_bound = saved_this;
        self.super_ctor = saved_super;
        self.instance_vars = saved_instances;
        self.scopes.leave_fn();
        r
    }

    /// `this.{name} = value` as a statement — the shape the walker's own
    /// member assignment lowers to.
    fn this_assign(&self, name: &str, value: CastExpr, span: Option<Span>) -> CastStmt {
        let m = self.meta(span);
        CastStmt::ExprStmt {
            expr: CastExpr::Call {
                function: "__crush_setindex__".to_string(),
                args: vec![
                    CastExpr::Var {
                        name: "this".to_string(),
                        meta: m.clone(),
                    },
                    CastExpr::StringLiteral {
                        value: name.to_string(),
                        meta: m.clone(),
                    },
                    value,
                ],
                meta: m.clone(),
            },
            meta: m,
        }
    }

    /// A constant expression, for a static field's initialiser: a
    /// literal (or a negated number), as the walker itself reads them.
    fn const_expr(&self, e: &Expression) -> Option<crush_cosm::Const> {
        match unparen(e) {
            Expression::Literal(l) => match l.kind() {
                LiteralKind::String(s) => Some(crush_cosm::Const::Str(self.name(*s))),
                LiteralKind::Int(i) => Some(crush_cosm::Const::Int(i64::from(*i))),
                LiteralKind::Num(n) if n.fract() == 0.0 && n.abs() < 9.223_372_036_854_776e18 => {
                    Some(crush_cosm::Const::Int(*n as i64))
                }
                LiteralKind::Num(n) => Some(crush_cosm::Const::Float(*n)),
                LiteralKind::Bool(b) => Some(crush_cosm::Const::Bool(*b)),
                LiteralKind::Null => Some(crush_cosm::Const::Null),
                _ => None,
            },
            Expression::Unary(u) if matches!(u.op(), UnaryOp::Minus) => {
                match self.const_expr(u.target())? {
                    crush_cosm::Const::Int(i) => Some(crush_cosm::Const::Int(-i)),
                    crush_cosm::Const::Float(f) => Some(crush_cosm::Const::Float(-f)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// The lifted function of `kind`'s static method `method`, when the
    /// class declares one (a static call's target is known exactly).
    pub(super) fn kind_static_fn(&self, kind: &str, method: &str) -> Option<String> {
        let k = self.kinds.iter().find(|k| k.name == kind)?;
        k.members
            .iter()
            .find(|m| m.name == method)
            .and_then(|m| match &m.member {
                Member::Static {
                    value: crush_cosm::StaticValue::Fn(f),
                } => Some(f.to_string()),
                _ => None,
            })
    }

    /// Emit one class as a KindDecl plus its lifted functions; returns the
    /// Kind's name (the binding's resolved crush name, so block-scoped
    /// classes get their fresh names).
    fn class_kind(
        &mut self,
        name: Option<Sym>,
        super_ref: Option<&Expression>,
        ctor: Option<&FunctionExpression>,
        elements: &[ClassElement],
        span: Option<Span>,
    ) -> anyhow::Result<String> {
        let kind = self.resolve(name.expect("class_kind callers name the class"));
        let parents = match super_ref {
            None => Vec::new(),
            Some(Expression::Identifier(id)) => vec![KindRef::new(self.resolve(id.sym()))],
            Some(_) => {
                return Err(self.unsupported(
                    "a class `extends` of anything but a name (a member or call expression)",
                    span,
                ));
            }
        };

        // `super.m()` in this class's members resolves from its parent up
        // (CRUSH-277 S5); the class itself is the [[HomeObject]] while
        // its member bodies lower.
        if let Some(parent) = parents.first() {
            self.class_parents
                .insert(kind.clone(), parent.as_str().to_string());
        }
        let saved_home = self.home_class.replace(kind.clone());

        let ctor_fn = format!("{kind}::constructor");
        // The kind registers before any member body lowers, so method and
        // field-initialiser bodies may construct the class itself
        // (`x = new C()`, `clone() { return new C(…); }`); its member
        // list is patched in at the end.
        let make_decl = |members: Vec<MemberDecl>| KindDecl {
            name: kind.clone(),
            home_lang: "javascript".to_string(),
            span: span
                .map(|s| crush_cosm::Span::new(s.start().line_number(), s.start().column_number())),
            storage: Storage::OpenDict,
            semantics: Semantics::Reference,
            parents: parents.clone(),
            resolution: Resolution::PrototypeChain,
            receiver: Receiver::DynamicThis,
            construct: Construct::JsNew {
                ctor_fn: FnRef::new(ctor_fn.clone()),
            },
            members,
            hooks: Hooks::default(),
            conforms: Vec::new(),
            equality: Equality::Identity,
        };
        let kind_idx = self.kinds.len();
        self.kinds.push(make_decl(Vec::new()));

        // Class fields (CRUSH-277 S5, D7): instance fields initialise on
        // the instance at construction (statements spliced into the
        // constructor below); a static field with a constant initialiser
        // is a constant of the kind.
        let mut field_stmts: Vec<CastStmt> = Vec::new();
        let mut static_members: Vec<MemberDecl> = Vec::new();
        for element in elements {
            match element {
                ClassElement::FieldDefinition(f) => {
                    let name = self.field_name(f.name(), span)?;
                    let value = self.field_init(f.initializer(), span)?;
                    field_stmts.push(self.this_assign(&name, value, span));
                }
                ClassElement::StaticFieldDefinition(f) => {
                    let name = self.field_name(f.name(), span)?;
                    let value = match f.initializer() {
                        Some(init) => self.const_expr(init).ok_or_else(|| {
                            self.unsupported(
                                "a non-constant static field initialiser",
                                Some(init.span()),
                            )
                        })?,
                        None => crush_cosm::Const::Null,
                    };
                    replace(
                        &mut static_members,
                        MemberDecl::new(
                            name,
                            Member::Static {
                                value: crush_cosm::StaticValue::Const(value),
                            },
                        ),
                    );
                }
                _ => {}
            }
        }

        match ctor {
            Some(f) => {
                // `super(...)` in this body calls the parent's constructor
                // on the same instance (CRUSH-277 S3).
                let saved = self.super_ctor.take();
                if let Some(parent) = parents.first() {
                    self.super_ctor = Some(format!("{}::constructor", parent.as_str()));
                }
                let lowered = self.method_function(f.parameters(), f.body(), span);
                self.super_ctor = saved;
                let (params, body) = lowered?;
                let parent_fn = parents
                    .first()
                    .map(|p| format!("{}::constructor", p.as_str()));
                let body = inject_fields(body, parent_fn.as_deref(), field_stmts.clone());
                self.functions.insert(
                    ctor_fn.clone(),
                    Function {
                        params,
                        body,
                        ..Default::default()
                    },
                );
            }
            // JS gives a base class an empty default constructor.
            None if parents.is_empty() => {
                let body = inject_fields(Vec::new(), None, field_stmts.clone());
                self.functions.insert(
                    ctor_fn.clone(),
                    Function {
                        params: vec![("this".to_string(), CastType::Any)],
                        body,
                        ..Default::default()
                    },
                );
            }
            // A derived class's default constructor forwards its arguments
            // to `super(...)`: synthesise it from the parent's constructor
            // signature, which the walker must have already lowered.
            None => {
                let parent = parents[0].as_str().to_string();
                let parent_ctor = format!("{parent}::constructor");
                let Some(parent_params) = self
                    .functions
                    .get(&parent_ctor)
                    .map(|f| f.params.clone())
                else {
                    return Err(self.unsupported(
                        format!(
                            "a derived class without a constructor whose parent `{parent}` isn't declared in this module (the default `super` forwarding needs its constructor)"
                        ),
                        span,
                    ));
                };
                let m = self.meta(span);
                let this_var = |meta: &Meta| CastExpr::Var {
                    name: "this".to_string(),
                    meta: meta.clone(),
                };
                let mut params = vec![("this".to_string(), CastType::Any)];
                let mut call_args = vec![this_var(&m)];
                for (name, ty) in parent_params.iter().skip(1) {
                    params.push((name.clone(), ty.clone()));
                    call_args.push(CastExpr::Var {
                        name: name.clone(),
                        meta: m.clone(),
                    });
                }
                let mut body = vec![CastStmt::ExprStmt {
                    expr: CastExpr::Call {
                        function: parent_ctor.clone(),
                        args: call_args,
                        meta: m.clone(),
                    },
                    meta: m,
                }];
                body = inject_fields(body, Some(&parent_ctor), field_stmts.clone());
                self.functions.insert(
                    ctor_fn.clone(),
                    Function {
                        params,
                        body,
                        ..Default::default()
                    },
                );
            }
        }

        let mut members: Vec<MemberDecl> = Vec::new();
        for element in elements {
            match element {
                ClassElement::MethodDefinition(m) => self.method_member(&kind, m, &mut members)?,
                // Collected above, before the constructor was lowered.
                ClassElement::FieldDefinition(_) | ClassElement::StaticFieldDefinition(_) => {}
                ClassElement::PrivateFieldDefinition(_)
                | ClassElement::PrivateStaticFieldDefinition(_) => {
                    return Err(self.unsupported("a private class field (`#x`)", span));
                }
                ClassElement::StaticBlock(_) => {
                    return Err(self.unsupported("a class `static { … }` block", span));
                }
            }
        }
        self.home_class = saved_home;
        members.extend(static_members);
        self.kinds[kind_idx].members = members;
        Ok(kind)
    }

    /// One method, accessor or static method: lift its body to a function
    /// and record the member. A later member of the same name and category
    /// replaces the earlier, as in JS.
    fn method_member(
        &mut self,
        kind: &str,
        m: &ClassMethodDefinition,
        members: &mut Vec<MemberDecl>,
    ) -> anyhow::Result<()> {
        if m.is_private() {
            return Err(self.unsupported("a private class method (`#m()`)", None));
        }
        let ClassElementName::PropertyName(prop) = m.name() else {
            return Err(self.unsupported("a private class method (`#m()`)", None));
        };
        let PropertyName::Literal(id) = prop else {
            return Err(self.unsupported("a computed member name (`[k]() {}`)", None));
        };
        let name = self.name(id.sym());
        match m.kind() {
            MethodDefinitionKind::Ordinary if m.is_static() => {
                let func = format!("{kind}::static::{name}");
                self.lift_static(kind, &func, m)?;
                replace(
                    members,
                    MemberDecl::new(
                        name,
                        Member::Static {
                            value: crush_cosm::StaticValue::Fn(FnRef::new(func)),
                        },
                    ),
                );
            }
            MethodDefinitionKind::Ordinary => {
                let func = format!("{kind}::{name}");
                self.lift_method(&func, m)?;
                replace(members, MemberDecl::method(name, func));
            }
            MethodDefinitionKind::Get => {
                let func = format!("{kind}::get::{name}");
                self.lift_method(&func, m)?;
                set_accessor(members, &name, Some(FnRef::new(func)), None);
            }
            MethodDefinitionKind::Set => {
                let func = format!("{kind}::set::{name}");
                self.lift_method(&func, m)?;
                set_accessor(members, &name, None, Some(FnRef::new(func)));
            }
            MethodDefinitionKind::Generator => {
                return Err(self.unsupported("a generator method (`*m()`) ", None));
            }
            MethodDefinitionKind::Async | MethodDefinitionKind::AsyncGenerator => {
                return Err(self.unsupported("an async method", None));
            }
        }
        Ok(())
    }

    /// Lower a method body into `Program::functions` under `func`, with
    /// `this` as its first parameter.
    fn lift_method(&mut self, func: &str, m: &ClassMethodDefinition) -> anyhow::Result<()> {
        let (params, body) = self.method_function(m.parameters(), m.body(), None)?;
        self.functions.insert(
            func.to_string(),
            Function {
                params,
                body,
                ..Default::default()
            },
        );
        Ok(())
    }

    /// Lower a static method body: plain parameters (the VM calls a
    /// static with no receiver), `this` bound to the class locally.
    fn lift_static(
        &mut self,
        kind: &str,
        func: &str,
        m: &ClassMethodDefinition,
    ) -> anyhow::Result<()> {
        let (params, body) = self.static_function(kind, m.parameters(), m.body(), None)?;
        self.functions.insert(
            func.to_string(),
            Function {
                params,
                body,
                ..Default::default()
            },
        );
        Ok(())
    }
}

/// Append `decl`, dropping any earlier member of the same name and
/// category (JS: the later definition wins).
fn replace(members: &mut Vec<MemberDecl>, decl: MemberDecl) {
    members.retain(|m| {
        !(m.name == decl.name
            && matches!(
                (&m.member, &decl.member),
                (Member::Method { .. }, Member::Method { .. })
                    | (Member::Static { .. }, Member::Static { .. })
            ))
    });
    members.push(decl);
}

/// Record one half of a `get`/`set` pair, merging with the other half
/// when it was declared first.
fn set_accessor(members: &mut Vec<MemberDecl>, name: &str, get: Option<FnRef>, set: Option<FnRef>) {
    if let Some(existing) = members
        .iter_mut()
        .find(|m| m.name == name && matches!(m.member, Member::Accessor { .. }))
    {
        if let Member::Accessor { get: g, set: s } = &mut existing.member {
            if get.is_some() {
                *g = get;
            }
            if set.is_some() {
                *s = set;
            }
        }
        return;
    }
    members.push(MemberDecl::new(
        name.to_string(),
        Member::Accessor { get, set },
    ));
}

/// Splice the field-initialisation statements into a constructor body
/// (CRUSH-277 S5, D7): JS runs them when the instance is built — at the
/// top of a base class's constructor, and in a derived class right
/// after the `super(...)` call (the parentCtor call on `this`). A
/// derived constructor without a visible `super(...)` call gets them
/// at the top; such a class is a JS error anyway (`this` before
/// `super`), so the placement can't mask a valid program.
fn inject_fields(
    mut body: Vec<CastStmt>,
    parent_ctor: Option<&str>,
    fields: Vec<CastStmt>,
) -> Vec<CastStmt> {
    if fields.is_empty() {
        return body;
    }
    let at = parent_ctor
        .and_then(|pc| {
            body.iter().position(|s| {
                matches!(s, CastStmt::ExprStmt { expr: CastExpr::Call { function, args, .. }, .. }
                    if function == pc
                        && matches!(args.first(), Some(CastExpr::Var { name, .. }) if name == "this"))
            })
        })
        .map_or(0, |i| i + 1);
    body.splice(at..at, fields);
    body
}
