//! COSM K1.3 (CRUSH-zorro-42): what the scheduler VM and the portable VM do
//! with a program that carries a kind table, until they implement Kinds
//! (K1.4, CRUSH-zorro-43).
//!
//! Neither VM has `Record`/`Kind` values yet: a struct instance is a map,
//! and a method is an ordinary function called by its qualified name
//! (`Point::norm2(p)`). For most of the strategy catalogue that is exactly
//! the declared behaviour — Crush's defaults (`reference` semantics,
//! `structural` equality, `literal` construction) are how maps already
//! behave, and the strategies that only govern member lookup and receiver
//! binding (storage, resolution, parents, receiver, conformances) are inert
//! while no call resolves on the receiver. Those programs run exactly as
//! they did before the kind table existed.
//!
//! A Kind that declares anything a map would *silently not honour* is
//! refused when the program is loaded, before any instruction runs:
//! - `value(…)` semantics (instances would be shared, not copied or moved);
//! - `identity` equality (maps compare structurally);
//! - a non-`literal` construction (no constructor would run);
//! - a protocol hook (`eq`, `str`, operators, …: maps ignore them);
//! - an accessor member (a field read would bypass the getter);
//! - a field default (a literal that omits the field would leave it unset).
//!
//! The Crush compiler only emits Kinds of the first kind (it refuses the
//! others at compile time, `crush_frontend::cosm_lower`); this gate is what
//! keeps a kind table from another producer — a walker, hand-written CASM, a
//! newer compiler — from running silently wrong.

use crate::vm::VmError;
use crush_cosm::{Construct, CosmModule, Equality, KindDecl, Member, Semantics, Strategy};

/// Why a Kind can't run as a map today (empty when it can).
fn unsupported_strategies(k: &KindDecl) -> Vec<String> {
    let mut why = Vec::new();
    if k.semantics != Semantics::Reference {
        why.push(format!("semantics: {}", k.semantics.as_str()));
    }
    if k.equality != Equality::Structural {
        why.push(format!("equality: {}", k.equality.as_str()));
    }
    if k.construct != Construct::Literal {
        why.push(format!("construct: {}", k.construct.strategy_name()));
    }
    if !k.hooks.is_empty() {
        why.push("protocol hooks".to_string());
    }
    for m in &k.members {
        if matches!(m.member, Member::Accessor { .. }) {
            why.push(format!("accessor `{}`", m.name));
        }
    }
    for f in k.declared_fields() {
        if f.default.is_some() {
            why.push(format!("a default for field `{}`", f.name));
        }
    }
    why
}

/// Why a kind table can't run on the scheduler VM or the portable VM yet,
/// naming each Kind and what it declares; `None` when every Kind behaves
/// exactly as today's maps.
pub fn unsupported_reason(module: Option<&CosmModule>) -> Option<String> {
    let module = module?;
    let problems: Vec<String> = module
        .kinds
        .iter()
        .filter_map(|k| {
            let why = unsupported_strategies(k);
            (!why.is_empty()).then(|| format!("`{}` declares {}", k.name, why.join(", ")))
        })
        .collect();
    if problems.is_empty() {
        return None;
    }
    Some(format!(
        "COSM objects aren't supported on this VM yet (CRUSH-zorro-43): {}; \
         without Kinds its instances would silently run as plain maps",
        problems.join("; ")
    ))
}

/// Load-time check of a kind table for the scheduler VM and the portable
/// VM: [`VmError::Unsupported`] with the [`unsupported_reason`], if any.
pub fn check_runnable(module: Option<&CosmModule>) -> Result<(), VmError> {
    match unsupported_reason(module) {
        Some(reason) => Err(VmError::Unsupported(reason)),
        None => Ok(()),
    }
}
