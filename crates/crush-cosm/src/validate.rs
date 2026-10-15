//! Compile-time validation of a [`CosmModule`].

use crate::mro::{KindLookup, Linearizer, ModuleSet, MroError};
use crate::schema::{
    Conformance, CosmModule, InterfaceDecl, KindDecl, KindRef, Member, MemberDecl, Semantics,
    Storage, ValueMode,
};
use crate::span::{Diagnostic, DiagnosticCode, Span};
use std::collections::HashSet;

/// Validate a module on its own. Every reference must resolve inside it.
pub fn validate(module: &CosmModule) -> Vec<Diagnostic> {
    validate_with(module, &[])
}

/// Validate a module whose kinds and interfaces may refer to declarations in
/// `deps` (e.g. a prelude declaring Python's `object`). Only `module` is
/// checked; `deps` are assumed valid. Diagnostics come in declaration order.
pub fn validate_with(module: &CosmModule, deps: &[&CosmModule]) -> Vec<Diagnostic> {
    let set = ModuleSet::new(module, deps);
    let mut v = Validator {
        set,
        lin: Linearizer::new(&set),
        out: Vec::new(),
    };
    v.duplicate_decls(module);
    for kind in &module.kinds {
        v.kind(kind);
    }
    for iface in &module.interfaces {
        v.interface(iface);
    }
    v.out
}

struct Validator<'s, 'a> {
    set: ModuleSet<'a>,
    lin: Linearizer<'s, ModuleSet<'a>>,
    out: Vec<Diagnostic>,
}

fn first_span<'s>(spans: &[Option<&'s Span>]) -> Option<&'s Span> {
    spans.iter().copied().flatten().next()
}

impl<'a> Validator<'_, 'a> {
    fn push(
        &mut self,
        code: DiagnosticCode,
        subject: String,
        message: String,
        span: Option<&Span>,
    ) {
        self.out.push(Diagnostic {
            code,
            subject,
            message,
            span: span.cloned(),
        });
    }

    fn duplicate_decls(&mut self, module: &CosmModule) {
        let mut seen: HashSet<&str> = HashSet::new();
        let decls = module
            .kinds
            .iter()
            .map(|k| (k.name.as_str(), "kind", k.span.as_ref()))
            .chain(
                module
                    .interfaces
                    .iter()
                    .map(|i| (i.name.as_str(), "interface", i.span.as_ref())),
            );
        for (name, what, span) in decls {
            if !seen.insert(name) {
                self.push(
                    DiagnosticCode::DuplicateDecl,
                    name.to_string(),
                    format!("{what} `{name}` is declared more than once in this module"),
                    span,
                );
            }
        }
    }

    fn kind(&mut self, kind: &'a KindDecl) {
        let kspan = kind.span.as_ref();
        let name = &kind.name;

        // storage
        let mut fields: HashSet<&str> = HashSet::new();
        for f in kind.storage.fields() {
            if !fields.insert(f.name.as_str()) {
                self.push(
                    DiagnosticCode::DuplicateField,
                    format!("{name}.{}", f.name),
                    format!("field `{}` is declared more than once", f.name),
                    first_span(&[f.span.as_ref(), kspan]),
                );
            }
        }
        if let (Semantics::Value(mode @ (ValueMode::Copy | ValueMode::Move)), true) =
            (kind.semantics, kind.storage.is_open())
        {
            self.push(
                DiagnosticCode::ValueSemanticsWithOpenStorage,
                name.clone(),
                format!(
                    "`value({})` semantics need a fixed layout, but storage is `{}`",
                    crate::catalogue::Strategy::as_str(mode),
                    kind.storage.strategy_name()
                ),
                kspan,
            );
        }

        // parents
        let mut parents_ok = true;
        for parent in &kind.parents {
            if self.set.lookup_kind(parent.as_str()).is_some() {
                continue;
            }
            parents_ok = false;
            if self.set.lookup_interface(parent.as_str()).is_some() {
                self.push(
                    DiagnosticCode::ParentIsInterface,
                    name.clone(),
                    format!(
                        "parent `{parent}` is an interface, not a kind; declare it in `conforms`"
                    ),
                    kspan,
                );
            } else {
                self.push(
                    DiagnosticCode::UnknownParent,
                    name.clone(),
                    format!("unknown parent kind `{parent}`"),
                    kspan,
                );
            }
        }

        // members
        let sealed = matches!(kind.storage, Storage::Sealed { .. });
        self.members(name, &kind.members, kspan);
        for m in &kind.members {
            let mspan = first_span(&[m.span.as_ref(), kspan]);
            if fields.contains(m.name.as_str()) && m.is_callable_or_static() && sealed {
                self.push(
                    DiagnosticCode::FieldShadowedByMember,
                    format!("{name}.{}", m.name),
                    format!(
                        "{} `{}` shadows the sealed field of the same name",
                        member_word(m),
                        m.name
                    ),
                    mspan,
                );
            }
            if matches!(m.member, Member::Field) && sealed && !fields.contains(m.name.as_str()) {
                self.push(
                    DiagnosticCode::UndeclaredField,
                    format!("{name}.{}", m.name),
                    format!(
                        "field member `{}` is not a declared field of sealed kind `{name}`",
                        m.name
                    ),
                    mspan,
                );
            }
        }

        // resolution order
        let order = if parents_ok { self.order(kind) } else { None };

        // conformances
        let mut seen_ifaces: HashSet<&str> = HashSet::new();
        for conf in &kind.conforms {
            if !seen_ifaces.insert(conf.interface.as_str()) {
                self.push(
                    DiagnosticCode::DuplicateConformance,
                    format!("{name}: {}", conf.interface),
                    format!(
                        "conformance to `{}` is declared more than once",
                        conf.interface
                    ),
                    first_span(&[conf.span.as_ref(), kspan]),
                );
                continue;
            }
            self.conformance(kind, conf, order.as_deref());
        }
    }

    /// The kind's resolution order, reporting failures that originate here.
    fn order(&mut self, kind: &KindDecl) -> Option<Vec<KindRef>> {
        let me = kind.kind_ref();
        let err = match self.lin.resolution_order(&me) {
            Ok(order) => return Some(order),
            Err(e) => e,
        };
        let code = match &err {
            MroError::NotLinear { .. } if err.kind() == &me => return None, // embedding: valid, not linear
            MroError::NotLinear { kind: parent, .. } => {
                let msg = format!(
                    "parent `{parent}` uses `embedding_promotion`, which has no linear order to inherit under `{}`",
                    crate::catalogue::Strategy::as_str(kind.resolution)
                );
                self.push(
                    DiagnosticCode::IncompatibleParent,
                    kind.name.clone(),
                    msg,
                    kind.span.as_ref(),
                );
                return None;
            }
            _ if err.kind() != &me => return None, // reported at its origin
            MroError::UnknownKind { .. } => return None, // reported as UnknownParent
            MroError::DuplicateBase { .. } => DiagnosticCode::DuplicateBase,
            MroError::Inconsistent { .. } => DiagnosticCode::InconsistentMro,
            MroError::Cycle { .. } => DiagnosticCode::InheritanceCycle,
            MroError::ParentArity { .. } => DiagnosticCode::ParentArity,
        };
        self.push(code, kind.name.clone(), err.to_string(), kind.span.as_ref());
        None
    }

    fn members(&mut self, owner: &str, members: &[MemberDecl], fallback: Option<&Span>) {
        let mut seen: HashSet<&str> = HashSet::new();
        for m in members {
            let mspan = first_span(&[m.span.as_ref(), fallback]);
            if !seen.insert(m.name.as_str()) {
                self.push(
                    DiagnosticCode::DuplicateMember,
                    format!("{owner}.{}", m.name),
                    format!("member `{}` is declared more than once", m.name),
                    mspan,
                );
            }
            if let Member::Accessor {
                get: None,
                set: None,
            } = m.member
            {
                self.push(
                    DiagnosticCode::EmptyAccessor,
                    format!("{owner}.{}", m.name),
                    format!("accessor `{}` has neither a getter nor a setter", m.name),
                    mspan,
                );
            }
        }
    }

    fn conformance(&mut self, kind: &KindDecl, conf: &Conformance, order: Option<&[KindRef]>) {
        let subject = format!("{}: {}", kind.name, conf.interface);
        let cspan = first_span(&[conf.span.as_ref(), kind.span.as_ref()]);
        let set = self.set;
        let Some(iface) = set.lookup_interface(conf.interface.as_str()) else {
            if self.set.lookup_kind(conf.interface.as_str()).is_some() {
                self.push(
                    DiagnosticCode::ConformsToKind,
                    subject,
                    format!(
                        "`{}` is a kind, not an interface; inherit from it in `parents`",
                        conf.interface
                    ),
                    cspan,
                );
            } else {
                self.push(
                    DiagnosticCode::UnknownInterface,
                    subject,
                    format!("unknown interface `{}`", conf.interface),
                    cspan,
                );
            }
            return;
        };
        if conf.check != iface.satisfaction {
            self.push(
                DiagnosticCode::SatisfactionMismatch,
                subject.clone(),
                format!(
                    "conformance is checked `{}` but interface `{}` is satisfied `{}`",
                    crate::catalogue::Strategy::as_str(conf.check),
                    iface.name,
                    crate::catalogue::Strategy::as_str(iface.satisfaction)
                ),
                cspan,
            );
        }
        self.members(&subject, &conf.members, cspan);
        for m in &conf.members {
            if iface.member(&m.name).is_none() {
                self.push(
                    DiagnosticCode::UnknownInterfaceMember,
                    format!("{subject}.{}", m.name),
                    format!("`{}` is not a member of interface `{}`", m.name, iface.name),
                    first_span(&[m.span.as_ref(), cspan]),
                );
            }
        }
        // Every interface member must be provided.
        let own = [kind.kind_ref()];
        let lineage: &[KindRef] = order.unwrap_or(&own);
        let mut missing: Vec<&str> = Vec::new();
        for im in &iface.members {
            if im.default.is_some() || self.provides(lineage, iface, &im.name) {
                continue;
            }
            missing.push(im.name.as_str());
        }
        if !missing.is_empty() {
            self.push(
                DiagnosticCode::MissingInterfaceMember,
                subject,
                format!(
                    "missing member(s) of interface `{}`: {}",
                    iface.name,
                    missing
                        .iter()
                        .map(|m| format!("`{m}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                cspan,
            );
        }
    }

    /// Whether some kind in `lineage` provides `member`: in its conformance
    /// to `iface`, or as a method/accessor/static member of its own.
    fn provides(&self, lineage: &[KindRef], iface: &InterfaceDecl, member: &str) -> bool {
        lineage
            .iter()
            .filter_map(|k| self.set.lookup_kind(k.as_str()))
            .any(|k| {
                k.conformance(&iface.name)
                    .is_some_and(|c| c.members.iter().any(|m| m.name == member))
                    || k.members
                        .iter()
                        .any(|m| m.name == member && m.is_callable_or_static())
            })
    }

    fn interface(&mut self, iface: &InterfaceDecl) {
        let mut seen: HashSet<&str> = HashSet::new();
        for m in &iface.members {
            if !seen.insert(m.name.as_str()) {
                self.push(
                    DiagnosticCode::DuplicateMember,
                    format!("{}.{}", iface.name, m.name),
                    format!("interface member `{}` is declared more than once", m.name),
                    first_span(&[m.span.as_ref(), iface.span.as_ref()]),
                );
            }
        }
    }
}

fn member_word(m: &MemberDecl) -> &'static str {
    match m.member {
        Member::Method { .. } => "method",
        Member::Accessor { .. } => "accessor",
        Member::Static { .. } => "static member",
        Member::Field => "field",
    }
}
