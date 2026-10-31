//! COSM K1.2 (CRUSH-zorro-41), phase 2: a checked [`Plan`] of Crush
//! declarations → a [`CosmModule`], with Crush's defaults, `@kind(…)`
//! overrides, and [`crush_cosm::validate`].
//!
//! **Crush's defaults** ([`KindDecl::crush`], design doc *Crush's own
//! KindDecl*): `sealed` storage with the declared fields, `reference`
//! semantics, resolution `none`, receiver `explicit_ref`, construct
//! `literal`, equality `structural`.
//!
//! **Shape of the lowering** for `struct P` + `impl P` + `impl T for P`:
//! - inherent methods (with `self`) → `Member::Method { fn: "P::m" }` using
//!   the Kind's receiver; members without `self` (`fn new`) →
//!   `Member::Static { fn: "P::new" }`;
//! - `impl T for P` → a nominal [`Conformance`] to `T` whose impl table names
//!   the trait's members as the interface does (`area` → `P::T::area`), so
//!   `crush_cosm::validate` checks it; **and** a qualified Kind member
//!   `T::area` per trait method (own impl or the trait's default
//!   `T::describe`), so the Kind lists everything callable on it;
//! - `trait T` → a nominal [`InterfaceDecl`]; a method with a body is a
//!   default (`default: "T::describe"`), one with `self` has receiver
//!   `explicit_ref`.

use std::collections::{BTreeSet, HashMap};

use anyhow::{Result, bail};
use crush_cast::{CastType, KindArg, KindValue};
use crush_cosm::{
    Conformance, Construct, CosmModule, CosmType, Equality, FieldDecl, FnRef, InterfaceDecl, InterfaceMember,
    KindDecl, KindRef, Member, MemberDecl, Param, Receiver, Resolution, Satisfaction, Semantics, Span,
    Storage, Strategy,
};
use serde_json::Value;

use crate::cosm_lower::{Plan, TypePlan, span_of, span_prefix};

/// Every key `@kind(…)` accepts, in the order the design doc lists them.
pub const KIND_KEYS: &[&str] =
    &["storage", "semantics", "parents", "resolution", "receiver", "construct", "equality"];

pub(crate) fn build_module(plan: &Plan) -> Result<CosmModule> {
    let mut errors = Vec::new();
    let mut module = CosmModule::new();
    for trait_plan in &plan.traits {
        module.interfaces.push(InterfaceDecl {
            name: trait_plan.name.clone(),
            home_lang: "crush".into(),
            span: span(&trait_plan.meta),
            members: trait_plan
                .methods
                .iter()
                .map(|m| InterfaceMember {
                    name: m.name.clone(),
                    span: span(&m.meta),
                    receiver: m.has_self().then_some(Receiver::ExplicitRef),
                    params: m
                        .params
                        .iter()
                        .skip(usize::from(m.has_self()))
                        .map(|(n, t)| Param { name: n.clone(), ty: cosm_type(t) })
                        .collect(),
                    returns: CosmType::Any,
                    default: m
                        .body
                        .as_ref()
                        .map(|_| FnRef::new(crate::cosm_lower::trait_default_fn_name(&trait_plan.name, &m.name))),
                })
                .collect(),
            satisfaction: Satisfaction::Nominal,
        });
    }
    for ty in &plan.types {
        match kind_decl(ty) {
            Ok(k) => module.kinds.push(k),
            Err(e) => errors.extend(e),
        }
    }
    if errors.is_empty() {
        // Spans in diagnostics point at the most specific declaration.
        for d in crush_cosm::validate(&module) {
            let at = d.span.as_ref().map(|s| format!("line {}, col {}: ", s.line, s.col)).unwrap_or_default();
            errors.push(format!("{at}{}: {} ({})", d.subject, d.message, d.code));
        }
    }
    if !errors.is_empty() {
        bail!("{}", errors.join("\n"));
    }
    Ok(module)
}

fn kind_decl(ty: &TypePlan) -> std::result::Result<KindDecl, Vec<String>> {
    let mut errors = Vec::new();
    let mut k = KindDecl::crush(ty.name.clone());
    k.span = span(&ty.meta);
    let fields: Vec<FieldDecl> = ty
        .fields
        .iter()
        .map(|(n, t)| FieldDecl::new(n.clone(), cosm_type(t)))
        .collect();
    k.storage = Storage::Sealed { fields: fields.clone() };

    for m in &ty.inherent {
        let member = if m.has_self {
            Member::Method { func: FnRef::new(m.function.clone()), receiver: None }
        } else {
            Member::Static { value: crush_cosm::StaticValue::Fn(FnRef::new(m.function.clone())) }
        };
        k.members.push(MemberDecl { name: m.name.clone(), span: span(&m.meta), member });
    }
    for ti in &ty.trait_impls {
        let mut conf = Conformance::new(ti.trait_name.as_str(), Satisfaction::Nominal);
        conf.span = span(&ti.meta);
        for m in &ti.members {
            let member = |name: String| {
                let member = if m.has_self {
                    Member::Method { func: FnRef::new(m.function.clone()), receiver: None }
                } else {
                    Member::Static { value: crush_cosm::StaticValue::Fn(FnRef::new(m.function.clone())) }
                };
                MemberDecl { name, span: span(&m.meta), member }
            };
            // `Shape::area` on the Kind; `area` in the conformance's impl table
            // (only the type's own implementations — defaults come from the
            // interface).
            k.members.push(member(m.name.clone()));
            let bare = m.name.rsplit("::").next().unwrap_or(&m.name).to_string();
            let is_default =
                m.function == crate::cosm_lower::trait_default_fn_name(&ti.trait_name, &bare);
            if !is_default {
                conf.members.push(member(bare));
            }
        }
        k.conforms.push(conf);
    }

    let Some(annotation) = &ty.kind else {
        return Ok(k);
    };
    let what = format!("`@kind` on struct `{}`", ty.name);
    let mut seen: HashMap<&str, &HashMap<String, Value>> = HashMap::new();
    let mut storage_name: Option<&str> = None;
    for arg in &annotation.args {
        let at = span_prefix(&arg.meta);
        if let Some(first) = seen.get(arg.key.as_str()) {
            let first = span_of(first).map(|(l, c)| format!("line {l}, col {c}")).unwrap_or_default();
            errors.push(format!("{at}{what}: `{}` is given twice (first at {first})", arg.key));
            continue;
        }
        seen.insert(&arg.key, &arg.meta);
        let r: std::result::Result<(), String> = (|| {
            match arg.key.as_str() {
                "storage" => {
                    let name = bare_name(arg)?;
                    let storage = Storage::from_name(&name).map_err(|e| e.to_string())?;
                    storage_name = Some(storage.strategy_name());
                    k.storage = match storage {
                        Storage::Sealed { .. } => Storage::Sealed { fields: fields.clone() },
                        Storage::OpenDictWithSlots { .. } => Storage::OpenDictWithSlots { slots: fields.clone() },
                        Storage::OpenDict => Storage::OpenDict,
                    };
                }
                "semantics" => k.semantics = Semantics::parse(&arg.value.to_string()).map_err(|e| e.to_string())?,
                "resolution" => k.resolution = Resolution::parse(&bare_name(arg)?).map_err(|e| e.to_string())?,
                "receiver" => k.receiver = Receiver::parse(&bare_name(arg)?).map_err(|e| e.to_string())?,
                "equality" => k.equality = Equality::parse(&bare_name(arg)?).map_err(|e| e.to_string())?,
                "parents" => {
                    let items = match &arg.value {
                        KindValue::List { items } => items.clone(),
                        single @ KindValue::Name { .. } => vec![single.clone()],
                        other => return Err(format!("`parents` takes a list of struct names, like `[Base, Mixin]`; got `{other}`")),
                    };
                    k.parents = items
                        .iter()
                        .map(|v| match v {
                            KindValue::Name { name } => Ok(KindRef::new(name.clone())),
                            other => Err(format!("`parents` takes struct names; `{other}` is not one")),
                        })
                        .collect::<std::result::Result<_, _>>()?;
                }
                "construct" => k.construct = construct(ty, &arg.value)?,
                other => {
                    return Err(format!(
                        "unknown `@kind` key `{other}`; valid keys: {}",
                        KIND_KEYS.join(", ")
                    ));
                }
            }
            Ok(())
        })();
        if let Err(e) = r {
            errors.push(format!("{at}{what}: {e}"));
        }
    }
    // `open_dict` keeps no declared layout: the struct's fields become
    // ordinary `field` members, so they stay part of the Kind.
    if storage_name == Some("open_dict") {
        for f in &fields {
            k.members.insert(0, MemberDecl { name: f.name.clone(), span: f.span.clone(), member: Member::Field });
        }
    }
    if errors.is_empty() { Ok(k) } else { Err(errors) }
}

/// A `@kind` value that must be a bare strategy name (`c3`, not `c3(x)`).
fn bare_name(arg: &KindArg) -> std::result::Result<String, String> {
    match &arg.value {
        KindValue::Name { name } => Ok(name.clone()),
        other => {
            let valid = match arg.key.as_str() {
                "storage" => crush_cosm::catalogue::STORAGE_NAMES,
                "resolution" => Resolution::NAMES,
                "receiver" => Receiver::NAMES,
                "equality" => Equality::NAMES,
                _ => &[],
            };
            Err(format!("`{}` takes one of: {}; got `{other}`", arg.key, valid.join(", ")))
        }
    }
}

/// `literal` · `ctor(f)` · `js_new(f)` · `py_new_init(init)` ·
/// `py_new_init(new, init)`. A function argument names a member of the
/// struct (`ctor(new)` → `P::new`) or a top-level function.
fn construct(ty: &TypePlan, value: &KindValue) -> std::result::Result<Construct, String> {
    let names = crush_cosm::catalogue::CONSTRUCT_NAMES.join(", ");
    let shapes = "`literal`, `ctor(f)`, `js_new(f)`, `py_new_init(init)` or `py_new_init(new, init)`";
    let fn_ref = |v: &KindValue| -> std::result::Result<FnRef, String> {
        let KindValue::Name { name } = v else {
            return Err(format!("a constructor function is a name; got `{v}`"));
        };
        Ok(FnRef::new(
            ty.inherent
                .iter()
                .find(|m| &m.name == name)
                .map(|m| m.function.clone())
                .unwrap_or_else(|| name.clone()),
        ))
    };
    match value {
        KindValue::Name { name } if name == "literal" => Ok(Construct::Literal),
        KindValue::Apply { name, args } => match (name.as_str(), args.as_slice()) {
            ("ctor", [f]) => Ok(Construct::Ctor { func: fn_ref(f)? }),
            ("js_new", [f]) => Ok(Construct::JsNew { ctor_fn: fn_ref(f)? }),
            ("py_new_init", [init]) => Ok(Construct::PyNewInit { new_fn: None, init_fn: Some(fn_ref(init)?) }),
            ("py_new_init", [new, init]) => {
                Ok(Construct::PyNewInit { new_fn: Some(fn_ref(new)?), init_fn: Some(fn_ref(init)?) })
            }
            _ => Err(format!("unknown construct strategy `{value}`; expected one of: {names} (written {shapes})")),
        },
        _ => Err(format!("unknown construct strategy `{value}`; expected one of: {names} (written {shapes})")),
    }
}

fn span(meta: &HashMap<String, Value>) -> Option<Span> {
    span_of(meta).map(|(l, c)| Span::new(l as u32, c as u32))
}

/// `CastType` → its COSM twin (same JSON shape by design, see
/// `crush_cosm::types`).
fn cosm_type(t: &CastType) -> CosmType {
    serde_json::to_value(t).ok().and_then(|v| serde_json::from_value(v).ok()).unwrap_or(CosmType::Any)
}

pub(crate) fn receiver_method_names(module: &CosmModule) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for k in &module.kinds {
        for m in &k.members {
            if matches!(m.member, Member::Method { .. }) {
                out.insert(m.name.rsplit("::").next().unwrap_or(&m.name).to_string());
            }
        }
    }
    out
}
