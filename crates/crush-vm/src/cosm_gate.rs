//! COSM: which kind tables the scheduler VM and the portable VM run.
//!
//! Every engine applies the same rule, defined once in casm
//! ([`casm::cosm_unsupported_reason_on`]), at the level of COSM it
//! implements. Since K1.4 (CRUSH-zorro-43) these two VMs implement Kinds
//! ([`casm::CosmSupport::Kinds`], see [`crate::cosm_rt`]): they run Records
//! with every strategy except the few the rule still lists (prototype chains
//! and Go embedding, a Python `__new__`, accessors, and hooks other than
//! `str` / `repr` / `clone`), which are refused when the program is loaded,
//! before any instruction runs. FastVM, the JIT and AOT apply the same rule
//! at [`casm::CosmSupport::Maps`].

use crate::vm::VmError;
use crush_cosm::CosmModule;

/// How the scheduler VM and the portable VM name themselves in a refusal.
const ENGINE: &str = crate::cosm_rt::ENGINE;

/// Why a kind table can't run on the scheduler VM or the portable VM;
/// `None` when it runs.
pub fn unsupported_reason(module: Option<&CosmModule>) -> Option<String> {
    casm::cosm_unsupported_reason_on(module?, ENGINE, casm::CosmSupport::Kinds)
}

/// Load-time check of a kind table for the scheduler VM and the portable
/// VM: [`VmError::Unsupported`] with the [`unsupported_reason`], if any.
pub fn check_runnable(module: Option<&CosmModule>) -> Result<(), VmError> {
    match unsupported_reason(module) {
        Some(reason) => Err(VmError::Unsupported(reason)),
        None => Ok(()),
    }
}
