//! CRUSH-362: the compiler's `RECEIVER_CAPABILITIES` list (dot-less host
//! capabilities invoked in method form on a declared receiver,
//! `xs.arr_slice(1, 3)`) must match the VM's capability registry — every
//! dot-less registry name is a receiver capability, and vice versa. The
//! frontend cannot depend on `crush-vm` (the DAG is
//! frontend → cast → casm → errors), so this cross-crate test is the guard:
//! when the registry gains a bare-name capability, the compiler list must
//! gain it too (or the method form silently becomes a `call_member` that a
//! map/struct resolves — and on an array dies as a type error).

use crush_frontend::compile_crush_source;

/// The names, read out of the compiled bytecode: for each candidate name,
/// `xs.<name>(1)` compiles to `cap_call "<name>"` exactly when the compiler
/// treats it as a receiver capability. Reading the list from behavior (not
/// reflection) keeps this test honest about what the compiler actually does.
fn known_receiver_capabilities() -> Vec<String> {
    let candidates = [
        "parallel_map",
        "append",
        "push",
        "arr_set",
        "arr_get",
        "arr_slice",
        "make_range",
        "io_print",
        "fs_read",
        "name_method",
    ];
    let mut out = Vec::new();
    for name in candidates {
        let src = format!("fn main() {{ let xs = [0]; xs.{name}(1); }}");
        let p = match compile_crush_source(&src) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let is_cap = p.functions["main"].body.iter().any(|i| {
            i.op == "cap_call" && i.args.get("name").and_then(|v| v.as_str()) == Some(name)
        });
        let is_member = p.functions["main"].body.iter().any(|i| {
            i.op == "call_member" && i.args.get("name").and_then(|v| v.as_str()) == Some(name)
        });
        assert!(
            is_cap || is_member,
            "{name}: neither cap_call nor call_member"
        );
        if is_cap {
            out.push(name.to_string());
        }
    }
    out
}

#[test]
fn method_call_receiver_capabilities_agree() {
    // Every dot-less name in the VM registry must reach a declared
    // receiver's method call as a `cap_call` named after the field, not as
    // `call_member`.
    let registry_bare: Vec<String> = crush_vm::caps::capabilities()
        .keys()
        .filter(|n| !n.contains('.'))
        .map(|n| n.to_string())
        .collect();
    assert!(!registry_bare.is_empty(), "registry lost its bare names?");

    let known = known_receiver_capabilities();
    for name in &registry_bare {
        assert!(
            known.contains(name),
            "registry capability `{name}` is dot-less but missing from the compiler's \
             RECEIVER_CAPABILITIES; `xs.{name}(…)` would stop being a capability call"
        );
    }
    for name in &known {
        assert!(
            registry_bare.contains(name),
            "compiler RECEIVER_CAPABILITIES names `{name}`, which is not a dot-less \
             capability in the VM registry"
        );
    }
}

#[test]
fn dotted_capability_names_are_never_receiver_capabilities() {
    // `fs.read(x)` (namespaced) is a capability call on its own name; a
    // dotted call on a declared variable with any other method still
    // resolves at run time (CRUSH-362) — the receiver-capability split is
    // about bare names only.
    let p = compile_crush_source("fn main() { let s = fs.read(\"x\") }").unwrap();
    let body = &p.functions["main"].body;
    assert!(
        body.iter()
            .any(|i| i.op == "cap_call" && i.args["name"] == "fs.read"),
        "{body:?}"
    );
    let p = compile_crush_source("fn main() { let h = { }; h.str_contains(\"x\") }").unwrap();
    let body = &p.functions["main"].body;
    assert!(
        body.iter()
            .any(|i| i.op == "call_member" && i.args["name"] == "str_contains"),
        "{body:?}"
    );
}
