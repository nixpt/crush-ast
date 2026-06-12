//! Capability registry for the standalone crush-vm.
//!
//! Only **portable** capabilities live here — i/o and string ops that mean the
//! same thing on every host. Chroma-local caps (frame.emit, bus.*) and
//! exosphere caps (mesh.*, vm.fork, …) are NOT registered; programs that
//! declare them will get `UnknownCap` at runtime.
//!
//! Hosts that embed crush-vm and want to expose additional capabilities should
//! use the `HostCaps` extension point rather than forking this registry.

use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct CapabilitySpec {
    pub name: &'static str,
    /// Fixed arg count, or `None` if variadic.
    pub argc: Option<usize>,
    /// Does calling this cap push a result onto the stack?
    pub returns: bool,
    pub summary: &'static str,
    /// Only a PRIVILEGED-level sandbox policy may grant.
    pub privileged: bool,
}

static REGISTRY: OnceLock<HashMap<&'static str, CapabilitySpec>> = OnceLock::new();

pub fn capabilities() -> &'static HashMap<&'static str, CapabilitySpec> {
    REGISTRY.get_or_init(|| {
        let mut m = HashMap::new();
        let mut reg = |s: CapabilitySpec| { m.insert(s.name, s); };
        // I/O
        reg(CapabilitySpec { name: "io.print",  argc: None,    returns: false, privileged: false, summary: "write args (concatenated) to output" });
        // String
        reg(CapabilitySpec { name: "str.concat", argc: None,    returns: true,  privileged: false, summary: "concatenate all args → string" });
        reg(CapabilitySpec { name: "str.len",    argc: Some(1), returns: true,  privileged: false, summary: "byte length of a string" });
        m
    })
}

/// Privileged cap namespace prefixes — caps with these prefixes require an
/// elevated sandbox grant even if registered by the host.
const PRIVILEGED_PREFIXES: &[&str] = &["net.", "fs.write", "wallet.", "vm.fork", "vm.exec"];

pub fn is_privileged(cap: &str) -> bool {
    if let Some(spec) = capabilities().get(cap) {
        return spec.privileged;
    }
    let base = cap.split(':').next().unwrap_or(cap);
    PRIVILEGED_PREFIXES.iter().any(|p| base.starts_with(p))
}
