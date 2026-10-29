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
        reg(CapabilitySpec {
            name: "io.write",
            argc: Some(1),
            returns: false,
            privileged: false,
            summary: "write one value's text to output, without a newline",
        });
        reg(CapabilitySpec {
            name: "py.print",
            argc: None,
            returns: false,
            privileged: false,
            summary: "Python print: space-separated args, True/False/None spelling",
        });
        reg(CapabilitySpec {
            name: "py.print_sep_end",
            argc: None,
            returns: false,
            privileged: false,
            summary: "Python print with sep= / end=: (sep, end, *values); null = default (CRUSH-zorro-38)",
        });
        reg(CapabilitySpec {
            name: "lang.marshal",
            argc: Some(2),
            returns: true,
            privileged: false,
            summary: "lang.marshal(value, name): copy a value across an inline-lowered polyglot block's boundary as JSON would; loud for a value JSON can't carry (CRUSH-272)",
        });
        reg(CapabilitySpec {
            name: "parallel_map",
            argc: Some(2),
            returns: true,
            privileged: false,
            summary: "parallel_map(items, f): f(item) for each item in its own VM (share-nothing), results in order (#175)",
        });
        reg(CapabilitySpec {
            name: "js.div",
            argc: Some(2),
            returns: true,
            privileged: false,
            summary: "JavaScript a / b: an exact int quotient stays an int, otherwise float",
        });
        reg(CapabilitySpec {
            name: "js.add",
            argc: Some(2),
            returns: true,
            privileged: false,
            summary: "JavaScript a + b: string concatenation with JS ToString when either side is a string, else numeric",
        });
        reg(CapabilitySpec {
            name: "js.truthy",
            argc: Some(1),
            returns: true,
            privileged: false,
            summary: "JavaScript truthiness (0, NaN, '', null, false are falsy; arrays and objects truthy)",
        });
        reg(CapabilitySpec {
            name: "js.loose_eq",
            argc: Some(2),
            returns: true,
            privileged: false,
            summary: "JavaScript a == b for primitives (abstract equality); objects by identity",
        });
        reg(CapabilitySpec {
            name: "js.strict_eq",
            argc: Some(2),
            returns: true,
            privileged: false,
            summary: "JavaScript a === b (strict equality): no coercion, different types are false, objects by identity",
        });
        reg(CapabilitySpec {
            name: "js.format",
            argc: None,
            returns: true,
            privileged: false,
            summary: "the line console.log(args...) prints: strings as-is, other values like node's util.inspect",
        });
        reg(CapabilitySpec {
            name: "conv.text",
            argc: Some(2),
            returns: true,
            privileged: false,
            summary: "value as text in a language style (\"js\" / \"rust\"): numbers as that language prints them",
        });
        reg(CapabilitySpec {
            name: "seq.slice",
            argc: Some(4),
            returns: true,
            privileged: false,
            summary: "Python slice target[start:stop:step] on a string or array (bounds/step may be null)",
        });
        reg(CapabilitySpec {
            name: "math.power",
            argc: Some(2),
            returns: true,
            privileged: false,
            summary: "a ** b: exact for int ** non-negative int, float otherwise",
        });
        reg(CapabilitySpec {
            name: "math.floordiv",
            argc: Some(2),
            returns: true,
            privileged: false,
            summary: "Python-style floor division (toward −∞)",
        });
        reg(CapabilitySpec {
            name: "math.floormod",
            argc: Some(2),
            returns: true,
            privileged: false,
            summary: "Python-style modulo (sign follows divisor)",
        });
        reg(CapabilitySpec {
            name: "math.truediv",
            argc: Some(2),
            returns: true,
            privileged: false,
            summary: "Python-style true division (always float)",
        });
        reg(CapabilitySpec {
            name: "io.read",
            argc: Some(0),
            returns: true,
            privileged: false,
            summary: "read one line from stdin, trimmed of trailing newline; empty string on EOF",
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
            summary: "set an array/map element at index; returns the modified container",
        });
        reg(CapabilitySpec {
            name: "arr_get",
            argc: None,
            returns: true,
            privileged: false,
            summary: "get an array/map element at index",
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
            argc: None,  // variadic: 1, 2, or 3 args
            returns: true,
            privileged: false,
            summary: "create an integer range [start..stop) with optional step",
        });
        reg(CapabilitySpec {
            name: "conv.chr",
            argc: Some(1),
            returns: true,
            privileged: false,
            summary: "convert a Unicode codepoint to a single-character string",
        });
        reg(CapabilitySpec {
            name: "conv.ord",
            argc: Some(1),
            returns: true,
            privileged: false,
            summary: "convert a single-character string to its Unicode codepoint",
        });
        m
    })
}

/// Privileged cap namespace prefixes — caps with these prefixes require an
/// elevated sandbox grant even if registered by the host. Every `fs.*` cap
/// that changes the filesystem is here, not just `fs.write` (CRUSH-151).
const PRIVILEGED_PREFIXES: &[&str] = &[
    "net.", "fs.write", "fs.mkdir", "fs.rm", "fs.cp", "fs.mv", "fs.touch", "wallet.", "vm.fork",
    "vm.exec",
];

pub fn is_privileged(cap: &str) -> bool {
    if let Some(spec) = capabilities().get(cap) {
        return spec.privileged;
    }
    let base = cap.split(':').next().unwrap_or(cap);
    PRIVILEGED_PREFIXES.iter().any(|p| base.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::is_privileged;

    #[test]
    fn filesystem_mutations_are_privileged_reads_are_not() {
        for cap in [
            "fs.write", "fs.mkdir", "fs.rm", "fs.cp", "fs.mv", "fs.touch",
        ] {
            assert!(is_privileged(cap), "{cap}");
        }
        for cap in [
            "fs.read", "fs.cat", "fs.ls", "fs.list", "fs.find", "fs.pwd", "fs.cd",
        ] {
            assert!(!is_privileged(cap), "{cap}");
        }
    }
}
