//! COSM K1.3 (CRUSH-zorro-42): what the scheduler VM and the portable VM do
//! with a program that carries a kind table, until they implement Kinds
//! (K1.4, CRUSH-zorro-43).
//!
//! Every engine applies the same rule, defined once in casm
//! ([`casm::cosm_unsupported_reason`]): a Kind whose instances behave exactly
//! as today's maps (Crush's defaults) runs as before; a Kind that declares
//! anything a map would silently not honour is refused when the program is
//! loaded, before any instruction runs. The Crush compiler only emits the
//! first kind (it refuses the others at compile time,
//! `crush_frontend::cosm_lower`); this gate keeps a kind table from another
//! producer — a walker, hand-written CASM, a newer compiler — from running
//! silently wrong.

use crate::vm::VmError;
use crush_cosm::CosmModule;

/// How the scheduler VM and the portable VM name themselves in a refusal.
const ENGINE: &str = "this VM";

/// Why a kind table can't run on the scheduler VM or the portable VM yet;
/// `None` when every Kind behaves exactly as today's maps.
pub fn unsupported_reason(module: Option<&CosmModule>) -> Option<String> {
    casm::cosm_unsupported_reason(module?, ENGINE)
}

/// Load-time check of a kind table for the scheduler VM and the portable
/// VM: [`VmError::Unsupported`] with the [`unsupported_reason`], if any.
pub fn check_runnable(module: Option<&CosmModule>) -> Result<(), VmError> {
    match unsupported_reason(module) {
        Some(reason) => Err(VmError::Unsupported(reason)),
        None => Ok(()),
    }
}
