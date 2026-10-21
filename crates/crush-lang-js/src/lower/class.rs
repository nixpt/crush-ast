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
//! lower to a CAST `MethodCall`. CRUSH-280 adds default and rest
//! parameters on members (constructors included): each class's member
//! signatures register before its bodies lower, and the call sites the
//! walker knows (`new`, `super(...)`, `super.m()`, statics,
//! `this.m()`/`c.m()` on tracked instances, `new C().m()`) adapt their
//! arguments to them. Everything outside the slice stays a
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
use boa_ast::declaration::{Declaration, ExportDeclaration};
use boa_ast::property::{MethodDefinitionKind, PropertyName};
use boa_ast::{Span, Spanned, Statement, StatementListItem};
use boa_interner::Sym;
use crush_cast::{CastType, Expression as CastExpr, Function, Statement as CastStmt};
use crush_cosm::{
    Construct, Equality, FnRef, Hooks, KindDecl, KindRef, Member, MemberDecl, Receiver, Resolution,
    Semantics, Storage,
};

use super::scope::{Sig, sig_of};
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

    /// May `new` construct `resolved` here (CRUSH-281)? A class whose
    /// declaration item has been lowered always may. A class only
    /// pre-registered so far may from code that runs after the whole
    /// statement list has — a function body, or a field initialiser —
    /// but not from a statement of the list itself: JS would hit the
    /// class's temporal dead zone there.
    pub(super) fn construct_kind(&self, resolved: &str) -> bool {
        self.is_kind(resolved)
            || ((self.fn_depth > 0 || self.deferred_new > 0)
                && self.pending_kinds.contains(resolved))
    }

    /// Register a class's member signatures (CRUSH-280): the
    /// constructor's (a derived class without one inherits its
    /// parent's), and every method's, accessor's and static's, keyed
    /// by lifted function name. Private and computed members are
    /// skipped; the member pass refuses them by name. Idempotent —
    /// the class's own lowering registers the same values again.
    fn register_class_sigs(
        &mut self,
        kind: &str,
        parents: &[KindRef],
        ctor: Option<&FunctionExpression>,
        elements: &[ClassElement],
    ) {
        let ctor_fn = format!("{kind}::constructor");
        if let Some(f) = ctor {
            self.fn_sigs.insert(ctor_fn.clone(), sig_of(f.parameters()));
        } else if parents.is_empty() {
            self.fn_sigs.insert(ctor_fn.clone(), Sig::default());
        } else if let Some(sig) = self
            .fn_sigs
            .get(&format!("{}::constructor", parents[0].as_str()))
            .cloned()
        {
            // The synthesised forwarding constructor takes the parent's
            // parameters, defaults included.
            self.fn_sigs.insert(ctor_fn.clone(), sig);
        }
        for element in elements {
            let ClassElement::MethodDefinition(m) = element else {
                continue;
            };
            if m.is_private() {
                continue;
            }
            let ClassElementName::PropertyName(PropertyName::Literal(id)) = m.name() else {
                continue;
            };
            let name = self.name(id.sym());
            let func = match (m.kind(), m.is_static()) {
                (MethodDefinitionKind::Ordinary, true) => format!("{kind}::static::{name}"),
                (MethodDefinitionKind::Ordinary, false) => format!("{kind}::{name}"),
                (MethodDefinitionKind::Get, _) => format!("{kind}::get::{name}"),
                (MethodDefinitionKind::Set, _) => format!("{kind}::set::{name}"),
                _ => continue,
            };
            self.fn_sigs.insert(func, sig_of(m.parameters()));
        }
    }

    /// Pre-register the classes a statement list declares (CRUSH-281):
    /// their kind names, parents and member signatures, before any
    /// function in the list is lowered. The class items themselves
    /// still lower in order (the temporal-dead-zone rule lives in
    /// [`Lower::construct_kind`]).
    pub(super) fn preregister_kinds(&mut self, items: &[StatementListItem]) {
        for item in items {
            match item {
                StatementListItem::Declaration(d) => match d.as_ref() {
                    Declaration::ClassDeclaration(c) => self.preregister_class(
                        Some(c.name().sym()),
                        c.super_ref(),
                        c.constructor(),
                        c.elements(),
                    ),
                    Declaration::Lexical(l) => {
                        for v in l.variable_list().as_ref() {
                            self.preregister_class_expr(v.init());
                        }
                    }
                    _ => {}
                },
                StatementListItem::Statement(s) => {
                    if let Statement::Var(v) = &**s {
                        for d in v.0.as_ref() {
                            self.preregister_class_expr(d.init());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Pre-register an exported class (CRUSH-281): same as
    /// [`Lower::preregister_kinds`], for declarations that ride an
    /// `export`.
    pub(super) fn preregister_export(&mut self, e: &ExportDeclaration) {
        match e {
            ExportDeclaration::Declaration(Declaration::ClassDeclaration(c)) => {
                self.preregister_class(
                    Some(c.name().sym()),
                    c.super_ref(),
                    c.constructor(),
                    c.elements(),
                );
            }
            ExportDeclaration::Declaration(Declaration::Lexical(l)) => {
                for v in l.variable_list().as_ref() {
                    self.preregister_class_expr(v.init());
                }
            }
            _ => {}
        }
    }

    fn preregister_class_expr(&mut self, init: Option<&Expression>) {
        if let Some(Expression::ClassExpression(c)) = init.map(unparen) {
            self.preregister_class(
                c.name().map(|n| n.sym()),
                c.super_ref(),
                c.constructor(),
                c.elements(),
            );
        }
    }

    fn preregister_class(
        &mut self,
        name: Option<Sym>,
        super_ref: Option<&Expression>,
        ctor: Option<&FunctionExpression>,
        elements: &[ClassElement],
    ) {
        let Some(name) = name else {
            return;
        };
        let kind = self.resolve(name);
        self.pending_kinds.insert(kind.clone());
        let parents = match super_ref {
            None => Vec::new(),
            Some(Expression::Identifier(id)) => vec![KindRef::new(self.resolve(id.sym()))],
            // A non-name superclass is refused when the class item
            // itself lowers.
            Some(_) => Vec::new(),
        };
        if let Some(parent) = parents.first() {
            self.class_parents
                .insert(kind.clone(), parent.as_str().to_string());
        }
        self.register_class_sigs(&kind, &parents, ctor, elements);
    }

    /// The kind `e` constructs, when `e` is `new C()` of a class this
    /// module declares.
    pub(super) fn new_kind(&self, e: &Expression) -> Option<String> {
        match unparen(e) {
            Expression::New(n) => match unparen(n.constructor()) {
                Expression::Identifier(id) => {
                    let kind = self.resolve(id.sym());
                    self.construct_kind(&kind).then_some(kind)
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// The signature of `kind`'s method `method`: the nearest definition
    /// up the parent chain wins, as at run time (CRUSH-280).
    pub(super) fn method_sig(&self, kind: &str, method: &str) -> Option<Sig> {
        self.chain_sig(kind, method, false)
    }

    /// The signature of `kind`'s static method `method`, resolved up
    /// the parent chain like `method_sig` (CRUSH-286 review #4).
    pub(super) fn static_method_sig(&self, kind: &str, method: &str) -> Option<Sig> {
        self.chain_sig(kind, method, true)
    }

    pub(super) fn chain_sig(&self, kind: &str, method: &str, is_static: bool) -> Option<Sig> {
        self.chain_target(kind, method, is_static)
            .map(|(_, sig)| sig)
    }

    /// The lifted function a chain-resolved member call targets,
    /// with its signature.
    pub(super) fn chain_target(
        &self,
        kind: &str,
        method: &str,
        is_static: bool,
    ) -> Option<(String, Sig)> {
        let sep = if is_static { "::static::" } else { "::" };
        let mut cur = Some(kind.to_string());
        let mut seen = std::collections::HashSet::new();
        while let Some(k) = cur {
            if !seen.insert(k.clone()) {
                break;
            }
            let func = format!("{k}{sep}{method}");
            if let Some(sig) = self.fn_sigs.get(&func) {
                return Some((func, sig.clone()));
            }
            cur = self.class_parents.get(&k).cloned();
        }
        None
    }

    /// Is `other` `kind` itself or a subclass of it in this module?
    fn is_subclass(&self, other: &str, kind: &str) -> bool {
        let mut cur = Some(other.to_string());
        let mut seen = std::collections::HashSet::new();
        while let Some(k) = cur {
            if k == kind {
                return true;
            }
            if !seen.insert(k.clone()) {
                break;
            }
            cur = self.class_parents.get(&k).cloned();
        }
        false
    }

    /// End-of-module checks on the member calls the walker emitted
    /// (CRUSH-286 review #1/#2). Adaptation is positional (hidden flags
    /// and the rest slot sit in fixed positions), so a call shaped for
    /// one signature can run *silently* wrong-shaped when run-time
    /// dispatch lands on another; both shapes are refused here, once
    /// every class and override is known:
    ///
    /// - a call adapted for kind `K`'s `m` dispatches on the receiver's
    ///   actual class, so every subclass of `K` that resolves `m` must
    ///   resolve it to the same signature;
    /// - a call passed raw (unknown receiver) may reach any member of
    ///   that name, so no member of that name may have default or rest
    ///   parameters.
    pub(super) fn check_member_calls(&self) -> anyhow::Result<()> {
        for call in &self.member_calls {
            match (&call.kind, &call.adapted) {
                (Some(kind), Some(sig)) => {
                    for decl in &self.kinds {
                        if !self.is_subclass(&decl.name, kind) {
                            continue;
                        }
                        let found = if call.is_static {
                            self.static_method_sig(&decl.name, &call.method)
                        } else {
                            self.method_sig(&decl.name, &call.method)
                        };
                        if let Some(found) = found
                            && &found != sig
                        {
                            return Err(self.unsupported(
                                format!(
                                    "the call `{}.{}(...)` is shaped for the signature of `{}`'s `{}`, but `{}` overrides it with a different one (the hidden default/rest slots would land in the wrong positions)",
                                    kind, call.method, kind, call.method, decl.name
                                ),
                                call.span,
                            ));
                        }
                    }
                }
                _ => {
                    let defaulted = self.fn_sigs.iter().any(|(f, s)| {
                        (f.ends_with(&format!("::{}", call.method))
                            || f.ends_with(&format!("::static::{}", call.method)))
                            && s.iter()
                                .any(|p| *p != super::scope::Param::Plain)
                    });
                    if defaulted {
                        return Err(self.unsupported(
                            format!(
                                "calling `.{}(...)` here, where the receiver's class is unknown, while a member `{}` in this module has default or rest parameters (the arguments could land in the hidden slots)",
                                call.method, call.method
                            ),
                            call.span,
                        ));
                    }
                }
            }
        }
        Ok(())
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
        if !self.construct_kind(&kind) {
            // A class declared later in this statement list is known
            // but not yet initialised: JS's temporal dead zone
            // (CRUSH-281). Anything else was never a class here.
            if self.pending_kinds.contains(&kind) {
                return Err(self.unsupported(
                    format!(
                        "`new {}` before its class declaration has run (the class is in its temporal dead zone)",
                        self.name(id.sym())
                    ),
                    span,
                ));
            }
            return Err(self.unsupported(
                format!(
                    "`new {}`: only a class declared in this module constructs yet",
                    self.name(id.sym())
                ),
                span,
            ));
        }
        // The constructor's parameters are known, so default/rest
        // parameters adapt here (CRUSH-280); the `construct` operation
        // then sees exactly the constructor's arity.
        let args = match self.fn_sigs.get(&format!("{kind}::constructor")).cloned() {
            Some(sig) => self.adapt_args(&sig, n.arguments(), &format!("new {kind}"), span)?,
            None => self.args(n.arguments())?,
        };
        Ok(CastExpr::Construct {
            kind,
            args,
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
                    // The target function is known exactly, so its
                    // default/rest parameters adapt (CRUSH-280).
                    let args = match self.fn_sigs.get(&f).cloned() {
                        Some(sig) => {
                            self.adapt_args(&sig, args, &format!("super.{method}"), span)?
                        }
                        None => self.args(args)?,
                    };
                    return Ok(CastExpr::Call {
                        function: f,
                        args,
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
                    let func = format!("{ancestor}::{method}");
                    let adapted = match self.fn_sigs.get(&func).cloned() {
                        Some(sig) => {
                            self.adapt_args(&sig, args, &format!("super.{method}"), span)?
                        }
                        None => self.args(args)?,
                    };
                    let args = std::iter::once(CastExpr::Var {
                        name: "this".to_string(),
                        meta: m.clone(),
                    })
                    .chain(adapted)
                    .collect();
                    return Ok(CastExpr::Call {
                        function: func,
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
        let saved_instances = std::mem::take(&mut self.instance_kinds);
        // A field initialiser runs at construction, after the whole
        // module: it may construct a class declared later (CRUSH-281).
        self.deferred_new += 1;
        let r = match init {
            Some(e) => self.expr(e),
            None => Ok(CastExpr::NullLiteral {
                meta: self.meta(span),
            }),
        };
        self.deferred_new -= 1;
        self.this_bound = saved_this;
        self.super_ctor = saved_super;
        self.instance_kinds = saved_instances;
        self.scopes.leave_fn();
        r
    }

    /// `this.{name} = value` as a field *definition* — JS class fields
    /// use [[Define]] semantics: the field lands on the instance
    /// directly, never through an inherited setter (CRUSH-285 review
    /// #6). The ordinary member assignment keeps [[Set]] semantics.
    fn this_assign(&self, name: &str, value: CastExpr, span: Option<Span>) -> CastStmt {
        let m = self.meta(span);
        CastStmt::ExprStmt {
            expr: CastExpr::Call {
                function: "__crush_deffield__".to_string(),
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
        if let Some(k) = self.kinds.iter().find(|k| k.name == kind) {
            return k
                .members
                .iter()
                .find(|m| m.name == method)
                .and_then(|m| match &m.member {
                    Member::Static {
                        value: crush_cosm::StaticValue::Fn(f),
                    } => Some(f.to_string()),
                    _ => None,
                });
        }
        // A class not lowered yet (CRUSH-281 pre-registration): its
        // signatures registered ahead, so the lifted name is known.
        let f = format!("{kind}::static::{method}");
        self.fn_sigs.contains_key(&f).then_some(f)
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

        // Member signatures register before any body of this class
        // lowers, so a member can call another (`this.m()`, `new C()`,
        // `C.f()`) whatever the declaration order (CRUSH-280).
        self.register_class_sigs(&kind, &parents, ctor, elements);

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
                if !field_stmts.is_empty() {
                    // A field initialiser is lowered in the class scope
                    // but spliced into the constructor, where a
                    // constructor parameter of the same name would
                    // capture its reads (CRUSH-285 review #6): refuse
                    // rather than read the parameter.
                    let param_names: std::collections::HashSet<&str> =
                        params.iter().map(|(n, _)| n.as_str()).collect();
                    if let Some(name) = cast_var_names(&field_stmts)
                        .iter()
                        .find(|n| n.as_str() != "this" && param_names.contains(n.as_str()))
                    {
                        return Err(self.unsupported(
                            format!(
                                "a class field initialiser uses `{name}`, which the constructor also declares as a parameter (the initialiser must see the outer `{name}`)"
                            ),
                            span,
                        ));
                    }
                    // Fields initialise right after a top-level
                    // `super(...)` call; one inside a branch would run
                    // them before the parent constructor (CRUSH-285
                    // review #6): refuse.
                    if let Some(pc) = parent_fn.as_deref() {
                        let top_level = body.iter().any(|s| {
                            matches!(s, CastStmt::ExprStmt { expr: CastExpr::Call { function, args, .. }, .. }
                                if function == pc
                                    && matches!(args.first(), Some(CastExpr::Var { name, .. }) if name == "this"))
                        });
                        if !top_level && cast_calls_function(&body, pc) {
                            return Err(self.unsupported(
                                "class fields with `super(...)` called from inside a branch (fields initialise after a top-level `super(...)` call)",
                                span,
                            ));
                        }
                    }
                }
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

/// Names of every `Var` read in lowered statements, via the CAST JSON
/// (the same `{"type": "Var"}` shape the frontend's scope scan walks).
fn cast_var_names(stmts: &[CastStmt]) -> std::collections::HashSet<String> {
    fn walk(v: &serde_json::Value, out: &mut std::collections::HashSet<String>) {
        match v {
            serde_json::Value::Object(o) => {
                if o.get("type").and_then(|t| t.as_str()) == Some("Var")
                    && let Some(n) = o.get("name").and_then(|n| n.as_str())
                {
                    out.insert(n.to_string());
                }
                o.values().for_each(|c| walk(c, out));
            }
            serde_json::Value::Array(a) => a.iter().for_each(|c| walk(c, out)),
            _ => {}
        }
    }
    let mut out = std::collections::HashSet::new();
    walk(
        &serde_json::to_value(stmts).unwrap_or(serde_json::Value::Null),
        &mut out,
    );
    out
}

/// Does a lowered statement list contain a call of `function` anywhere
/// (inside branches and expressions included)?
fn cast_calls_function(stmts: &[CastStmt], function: &str) -> bool {
    fn walk(v: &serde_json::Value, function: &str) -> bool {
        match v {
            serde_json::Value::Object(o) => {
                (o.get("type").and_then(|t| t.as_str()) == Some("Call")
                    && o.get("function").and_then(|f| f.as_str()) == Some(function))
                    || o.values().any(|c| walk(c, function))
            }
            serde_json::Value::Array(a) => a.iter().any(|c| walk(c, function)),
            _ => false,
        }
    }
    walk(
        &serde_json::to_value(stmts).unwrap_or(serde_json::Value::Null),
        function,
    )
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
