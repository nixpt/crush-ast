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
        let mut reg = |s: CapabilitySpec| {
            m.insert(s.name, s);
        };
        // I/O
        reg(CapabilitySpec {
            name: "io.print",
            argc: None,
            returns: false,
            privileged: false,
            summary: "write args (concatenated) to output",
        });
        // String
        reg(CapabilitySpec {
            name: "str.concat",
            argc: None,
            returns: true,
            privileged: false,
            summary: "concatenate all args → string",
        });
        reg(CapabilitySpec {
            name: "str.len",
            argc: Some(1),
            returns: true,
            privileged: false,
            summary: "byte length of a string",
        });
        reg(CapabilitySpec {
            name: "str.contains",
            argc: Some(2),
            returns: true,
            privileged: false,
            summary: "check if string contains a substring",
        });
        reg(CapabilitySpec {
            name: "str.split",
            argc: Some(2),
            returns: true,
            privileged: false,
            summary: "split string by delimiter",
        });
        reg(CapabilitySpec {
            name: "str.replace",
            argc: Some(3),
            returns: true,
            privileged: false,
            summary: "replace all occurrences",
        });
        reg(CapabilitySpec {
            name: "str.join",
            argc: Some(2),
            returns: true,
            privileged: false,
            summary: "join array elements with delimiter",
        });
                reg(CapabilitySpec {
            name: "append",
            argc: None,
            returns: true,
            privileged: false,
            summary: "append an element to an array; returns the modified array",
        });
        reg(CapabilitySpec {
            name: "push",
            argc: None,
            returns: true,
            privileged: false,
            summary: "push an element onto an array; returns the modified array",
        });
        reg(CapabilitySpec {
            name: "arr_set",
            argc: None,
            returns: true,
            privileged: false,
            summary: "set an array element at index; returns the modified array",
        });
        reg(CapabilitySpec {
            name: "arr_get",
            argc: None,
            returns: true,
            privileged: false,
            summary: "get an array element at index",
        });
        reg(CapabilitySpec {
            name: "arr_slice",
            argc: Some(3),
            returns: true,
            privileged: false,
            summary: "slice an array [start..end) — null start=0, null end=len",
        });
        reg(CapabilitySpec {
            name: "make_range",
            argc: None,  // variadic: 0, 1, or 2 args
            returns: true,
            privileged: false,
            summary: "create an integer range [start..end)",
        });
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
