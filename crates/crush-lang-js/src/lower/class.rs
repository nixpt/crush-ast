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
//! This slice only *emits*: construction, method calls and loading the
//! class as a value arrive with S3, and nothing runs until the VM slice
//! (S2) lifts the prototype-chain gate — until then the frontend refuses
//! these programs loudly, naming the strategy. Everything outside the
//! slice stays a typed refusal here: class fields and static fields (S5),
//! `super` (S3/S5), private members, static blocks, computed member names,
//! async/generator methods, anonymous class expressions, expression
//! superclasses, and a derived class with no constructor (its JS default
//! forwards to `super`, S3).

use boa_ast::expression::Expression;
use boa_ast::function::{
    ClassDeclaration, ClassElement, ClassElementName, ClassExpression, ClassMethodDefinition,
    FunctionExpression,
};
use boa_ast::property::{MethodDefinitionKind, PropertyName};
use boa_ast::{Span, Spanned};
use boa_interner::Sym;
use crush_cast::{Function, Statement as CastStmt};
use crush_cosm::{
    Construct, Equality, FnRef, Hooks, KindDecl, KindRef, Member, MemberDecl, Receiver, Resolution,
    Semantics, Storage,
};

use super::Lower;

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

        let ctor_fn = format!("{kind}::constructor");
        match ctor {
            Some(f) => {
                let (params, body) = self.method_function(f.parameters(), f.body(), span)?;
                self.functions.insert(
                    ctor_fn.clone(),
                    Function {
                        params,
                        body,
                        ..Default::default()
                    },
                );
            }
            // JS gives a base class an empty default constructor. A derived
            // class's default forwards its arguments to `super(...)` —
            // S3; refusing here keeps the emitted shape honest until then.
            None if parents.is_empty() => {
                self.functions.insert(
                    ctor_fn.clone(),
                    Function {
                        params: vec![("this".to_string(), crush_cast::CastType::Any)],
                        body: Vec::new(),
                        ..Default::default()
                    },
                );
            }
            None => {
                return Err(self.unsupported(
                    "a derived class without a constructor (the default `super` forwarding arrives with CRUSH-277 S3)",
                    span,
                ));
            }
        }

        let mut members: Vec<MemberDecl> = Vec::new();
        for element in elements {
            match element {
                ClassElement::MethodDefinition(m) => self.method_member(&kind, m, &mut members)?,
                ClassElement::FieldDefinition(_) | ClassElement::StaticFieldDefinition(_) => {
                    return Err(self.unsupported(
                        "a class field (`x = …` in a class body; arrives with CRUSH-277 S5)",
                        span,
                    ));
                }
                ClassElement::PrivateFieldDefinition(_)
                | ClassElement::PrivateStaticFieldDefinition(_) => {
                    return Err(self.unsupported("a private class field (`#x`)", span));
                }
                ClassElement::StaticBlock(_) => {
                    return Err(self.unsupported("a class `static { … }` block", span));
                }
            }
        }

        self.kinds.push(KindDecl {
            name: kind.clone(),
            home_lang: "javascript".to_string(),
            span: span
                .map(|s| crush_cosm::Span::new(s.start().line_number(), s.start().column_number())),
            storage: Storage::OpenDict,
            semantics: Semantics::Reference,
            parents,
            resolution: Resolution::PrototypeChain,
            receiver: Receiver::DynamicThis,
            construct: Construct::JsNew {
                ctor_fn: FnRef::new(ctor_fn),
            },
            members,
            hooks: Hooks::default(),
            conforms: Vec::new(),
            equality: Equality::Identity,
        });
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
                self.lift_method(&func, m)?;
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
