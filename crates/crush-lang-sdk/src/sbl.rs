//! System Bytecode Layer (SBL) — `system.*` capabilities implemented in Crush.
//!
//! Ported from exosphere's nanovm (`src/sbl_core.{crush,casm}`, W10 /
//! CRUSH-122). nanovm's idea was to keep security-relevant system logic
//! (path normalization first) out of host Rust and in Crush bytecode, so
//! every caller shares one implementation that runs under VM quotas. nanovm
//! never wired it up — its `sbl_core.casm` was a hand-written stub whose
//! `path_normalize` returned its input unchanged. Here the Crush source in
//! `sbl/sbl_core.crush` is the implementation:
//!
//! 1. it is compiled once, on first use, through the normal
//!    `compile_crush_source` pipeline;
//! 2. every function it defines is registered as `system.<name>`, with the
//!    argument count taken from its declared parameters;
//! 3. each call runs that function in a fresh [`PortableVm`] whose only host
//!    capabilities are the pure stdlib families — no fs/net/env/process, so
//!    the SBL cannot reach anything the stdlib itself cannot.
//!
//! Registered from [`crate::stdlib::register`], i.e. behind `--stdlib`.

use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps, PortableVm, Program, Quotas};
use std::collections::HashMap;
use std::sync::OnceLock;

/// The SBL's Crush source, shipped inside the crate.
pub const SBL_CORE_SOURCE: &str = include_str!("../sbl/sbl_core.crush");

/// Capability namespace the SBL functions are exposed under.
const NAMESPACE: &str = "system";

struct SblLibrary {
    program: Program,
    /// function name -> declared parameter count
    arity: HashMap<String, usize>,
}

fn library() -> Result<&'static SblLibrary, String> {
    static LIB: OnceLock<Result<SblLibrary, String>> = OnceLock::new();
    LIB.get_or_init(|| {
        let cast = crush_frontend::parse_source(SBL_CORE_SOURCE)
            .map_err(|e| format!("sbl_core.crush: parse failed: {e}"))?;
        let arity = cast
            .functions
            .iter()
            .map(|(name, f)| (name.clone(), f.params.len()))
            .collect();
        let program = crate::compile::compile_crush_source(SBL_CORE_SOURCE)
            .map_err(|e| format!("sbl_core.crush: compile failed: {e}"))?;
        Ok(SblLibrary { program, arity })
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// Names of the functions the SBL exports, sorted (without the `system.`
/// prefix). Empty if the embedded source fails to compile.
pub fn functions() -> Vec<String> {
    let mut names: Vec<String> = library()
        .map(|lib| lib.arity.keys().cloned().collect())
        .unwrap_or_default();
    names.sort();
    names
}

/// Call one SBL function directly, outside any enclosing program.
pub fn call(function: &str, args: Vec<Value>) -> Result<Value, String> {
    let lib = library()?;
    let expected = *lib
        .arity
        .get(function)
        .ok_or_else(|| format!("{NAMESPACE}.{function}: no such SBL function"))?;
    if args.len() != expected {
        return Err(format!(
            "{NAMESPACE}.{function}: expected {expected} argument(s), got {}",
            args.len()
        ));
    }

    let mut program = lib.program.clone();
    program.manifest.entry = Some(function.to_string());

    let mut caps = HostCaps::new();
    crate::stdlib::register_pure(&mut caps);

    let mut vm = PortableVm::new(program);
    vm.set_quotas(Quotas::default());
    vm.set_host_caps(caps);
    vm.push_entry_args(args);
    let mut result = vm
        .run()
        .map_err(|e| format!("{NAMESPACE}.{function}: {e}"))?;
    Ok(result.stack.pop().unwrap_or(Value::Null))
}

/// Register every SBL function as a `system.<name>` capability. If the
/// embedded source fails to compile, nothing is registered and the error is
/// reported on stderr (the rest of the stdlib is unaffected).
pub fn register(caps: &mut HostCaps) {
    match library() {
        Ok(lib) => {
            for (name, &argc) in &lib.arity {
                caps.register(Box::new(SblCap {
                    function: name.clone(),
                    argc,
                }));
            }
        }
        Err(e) => eprintln!("crush-lang-sdk: SBL disabled: {e}"),
    }
}

struct SblCap {
    function: String,
    argc: usize,
}

impl HostCap for SblCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: format!("{NAMESPACE}.{}", self.function),
            argc: Some(self.argc),
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        call(&self.function, args).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> Value {
        Value::Str(v.to_string())
    }

    #[test]
    fn sbl_source_compiles_and_exports_its_functions() {
        assert_eq!(
            functions(),
            vec!["format_info", "format_node_info", "path_normalize"]
        );
    }

    #[test]
    fn path_normalize_collapses_dot_dotdot_and_empty_segments() {
        let cases = [
            (
                "/mnt/data/../user/./projects//exosphere",
                "/mnt/user/projects/exosphere",
            ),
            ("/", "/"),
            ("", "/"),
            ("/../..", "/"),
            ("a/b/../c", "/a/c"),
            ("/a/./b/./", "/a/b"),
        ];
        for (input, expected) in cases {
            assert_eq!(
                call("path_normalize", vec![s(input)]),
                Ok(s(expected)),
                "input {input:?}"
            );
        }
    }

    #[test]
    fn format_info_and_format_node_info_agree() {
        let args = || vec![s("node-001"), s("1.2.3")];
        let expected = Ok(s("Node: node-001 (v1.2.3)"));
        assert_eq!(call("format_node_info", args()), expected);
        assert_eq!(call("format_info", args()), expected);
    }

    #[test]
    fn wrong_arity_and_unknown_function_are_errors() {
        assert!(
            call("path_normalize", vec![])
                .unwrap_err()
                .contains("expected 1")
        );
        assert!(
            call("nope", vec![])
                .unwrap_err()
                .contains("no such SBL function")
        );
    }

    #[test]
    fn registered_caps_carry_the_declared_arity() {
        let mut caps = HostCaps::new();
        register(&mut caps);
        let cap = caps
            .get("system.format_info")
            .expect("system.format_info registered");
        assert_eq!(cap.spec().argc, Some(2));
        assert!(caps.get("system.path_normalize").is_some());
    }
}
