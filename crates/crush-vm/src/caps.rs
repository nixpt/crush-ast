//! Capability registry — single source of truth for what ChromaVM programs can call.
//!
//! Portable capabilities (io.print / str.*) are the cross-VM shared subset;
//! frame.emit / bus.* are chroma-local. Non-portable caps should not appear in
//! programs intended to run on nanovm.

use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct CapabilitySpec {
    pub name: &'static str,
    /// Fixed arg count, or None if variadic.
    pub argc: Option<usize>,
    /// Does the cap push a result onto the stack?
    pub returns: bool,
    /// Part of the portable cross-VM subset?
    pub portable: bool,
    pub summary: &'static str,
    /// Requires a host bus to be provided to run().
    pub needs_bus: bool,
    /// Only a PRIVILEGED-level sandbox policy may grant.
    pub privileged: bool,
}

static REGISTRY: OnceLock<HashMap<&'static str, CapabilitySpec>> = OnceLock::new();

pub fn capabilities() -> &'static HashMap<&'static str, CapabilitySpec> {
    REGISTRY.get_or_init(|| {
        let mut m = HashMap::new();
        let mut reg = |s: CapabilitySpec| { m.insert(s.name, s); };
        reg(CapabilitySpec { name: "io.print",   argc: None,    returns: false, portable: true,  needs_bus: false, privileged: false, summary: "write args (concatenated) to output" });
        reg(CapabilitySpec { name: "str.concat",  argc: None,    returns: true,  portable: true,  needs_bus: false, privileged: false, summary: "concatenate all args → string" });
        reg(CapabilitySpec { name: "str.len",     argc: Some(1), returns: true,  portable: true,  needs_bus: false, privileged: false, summary: "length of a string" });
        reg(CapabilitySpec { name: "frame.emit",  argc: Some(1), returns: false, portable: false, needs_bus: false, privileged: false, summary: "emit a visual frame (chroma-local)" });
        reg(CapabilitySpec { name: "bus.send",    argc: Some(2), returns: false, portable: false, needs_bus: true,  privileged: false, summary: "send (topic, payload) on the chroma bus" });
        reg(CapabilitySpec { name: "bus.recv",    argc: Some(1), returns: true,  portable: false, needs_bus: true,  privileged: false, summary: "receive from a chroma bus topic" });
        m
    })
}

/// Convenience re-export matching the Python module's name.
pub use capabilities as CAPABILITIES;

pub static PORTABLE_CAPS: OnceLock<Vec<&'static str>> = OnceLock::new();

pub fn portable_caps() -> &'static [&'static str] {
    PORTABLE_CAPS.get_or_init(|| {
        capabilities().values().filter(|s| s.portable).map(|s| s.name).collect()
    })
}

const PRIVILEGED_PREFIXES: &[&str] = &[
    "net.", "fs.write", "wallet.", "vm.fork", "vm.exec", "mesh.",
];

pub fn is_privileged(cap: &str) -> bool {
    if let Some(spec) = capabilities().get(cap) {
        return spec.privileged;
    }
    let base = cap.split(':').next().unwrap_or(cap);
    PRIVILEGED_PREFIXES.iter().any(|p| base.starts_with(p))
}

// Alias for ergonomic re-export in lib.rs.
pub struct CapabilitySpecAlias;
