//! Regression test for CRUSH-16 / CRUSH-WORKSPACE-TEST-1.
//!
//! Cargo drops the `-C extra-filename=-<hash>` suffix for any target whose
//! `crate-type` includes a non-rlib output (`cdylib`, `staticlib`, ...), so
//! that target's rlib is written to ONE fixed path — e.g.
//! `target/debug/deps/libcrush_vm.rlib`.
//!
//! That is harmless for a leaf crate nothing depends on as a library. It is
//! corrupting for a crate that IS a normal library dependency, because this
//! workspace builds some of those crates in two graphs at once (the target
//! graph, plus a host graph via `crush-macros`, a proc-macro crate whose
//! tests depend on `crush-vm`). Both units write that same path, last writer
//! wins, and consumers silently link whichever landed — producing
//! `E0308: expected casm::Program, found a different casm::Program` in
//! `crush-lang-sdk`, or intermittently `E0463: can't find crate for ...`.
//!
//! Why this test exists rather than relying on the `Test (workspace)` CI job:
//! that failure is a **race**, so a whole-workspace build can go green by
//! winning the coin flip. This check is deterministic — it fails on the
//! manifest, before anything is compiled.
//!
//! History: `crush-vm` was `crate-type = ["lib", "cdylib"]`; CRUSH-16 set it
//! to `["lib"]` and moved the C ABI to `crush-vm-capi`; the PyO3 wheel commit
//! `4137646` re-added the `cdylib` and silently reverted that fix. The
//! bindings now live in `crush-vm-py`.
//!
//! **If this test fails:** do not relax it. Move the `cdylib`/`staticlib`
//! output into its own leaf crate (see `crush-vm-capi`, `crush-python`,
//! `crush-vm-py` for three worked examples) and depend on the plain lib.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    // crates/crush-vm/ -> crates/ -> <root>
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root is two levels above crates/crush-vm")
        .to_path_buf()
}

/// Crates that are allowed to emit a non-rlib output, because nothing in the
/// workspace depends on them as a normal Rust library — they are leaves whose
/// entire purpose is the shared object.
///
/// Adding a name here is a claim that the crate has NO in-workspace library
/// dependents. `no_dylib_crate_is_a_library_dependency` below verifies that
/// claim rather than trusting it.
const ALLOWED_NON_RLIB_LEAVES: &[&str] = &[
    "crush-vm-capi",       // C ABI  -> libcrush_vm_capi.so
    "crush-python",        // PyO3   -> crush-cast bindings
    "crush-vm-py",         // PyO3   -> the `crush_vm` wheel
    "crush-plugin-example",// FFI plugin sample loaded via libloading
    "crush-web",           // wasm32 target; excluded from the workspace anyway
];

/// Every member manifest, as (crate name, raw text).
fn member_manifests() -> Vec<(String, String)> {
    let root = workspace_root();
    let crates_dir = root.join("crates");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&crates_dir).expect("read crates/") {
        let entry = entry.expect("dir entry");
        let manifest = entry.path().join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(&manifest).expect("read manifest");
        let name = text
            .lines()
            .find_map(|l| {
                let l = l.trim();
                l.strip_prefix("name")
                    .and_then(|r| r.trim_start().strip_prefix('='))
                    .map(|r| r.trim().trim_matches('"').to_string())
            })
            .unwrap_or_else(|| {
                entry.file_name().to_string_lossy().to_string()
            });
        out.push((name, text));
    }
    out
}

/// Returns the `crate-type = [...]` entries declared in a manifest's `[lib]`.
fn declared_crate_types(manifest: &str) -> Option<Vec<String>> {
    let line = manifest
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("crate-type"))?;
    let inner = line.split('[').nth(1)?.split(']').next()?;
    Some(
        inner
            .split(',')
            .map(|s| s.trim().trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect(),
    )
}

#[test]
fn crush_vm_is_a_plain_lib() {
    let manifests = member_manifests();
    let (_, crush_vm) = manifests
        .iter()
        .find(|(n, _)| n == "crush-vm")
        .expect("crush-vm manifest");

    let types = declared_crate_types(crush_vm)
        .expect("crush-vm declares an explicit [lib] crate-type");

    assert_eq!(
        types,
        vec!["lib".to_string()],
        "crush-vm must stay `crate-type = [\"lib\"]`.\n\
         Found: {types:?}\n\n\
         A non-rlib crate-type costs this crate its `-C extra-filename` hash, so its\n\
         rlib collides at `deps/libcrush_vm.rlib` with the host-graph unit built for\n\
         `crush-macros`' tests. Consumers then link a `crush_vm` compiled against a\n\
         different `casm` than their `crush_frontend` (E0308 in crush-lang-sdk).\n\
         This is CRUSH-16, which regressed once already via the PyO3 wheel commit.\n\n\
         Put the cdylib in its own leaf crate instead — see crush-vm-py / crush-vm-capi."
    );
}

#[test]
fn no_dylib_crate_is_a_library_dependency() {
    let manifests = member_manifests();

    // Crates that emit a non-rlib output.
    let non_rlib: BTreeSet<String> = manifests
        .iter()
        .filter(|(_, text)| {
            declared_crate_types(text)
                .map(|t| t.iter().any(|c| c != "lib" && c != "rlib"))
                .unwrap_or(false)
        })
        .map(|(n, _)| n.clone())
        .collect();

    // Any such crate must be on the allow-list...
    for name in &non_rlib {
        assert!(
            ALLOWED_NON_RLIB_LEAVES.contains(&name.as_str()),
            "`{name}` declares a non-rlib crate-type but is not a known leaf.\n\
             Either give it its own leaf crate for the cdylib/staticlib, or add it to\n\
             ALLOWED_NON_RLIB_LEAVES here after confirming nothing depends on it as a\n\
             library. See this file's header for why this matters."
        );
    }

    // ...and must genuinely have no in-workspace library dependents, or the
    // allow-list entry is a lie and the collision hazard is live again.
    for name in &non_rlib {
        let dependents: Vec<&str> = manifests
            .iter()
            .filter(|(other, text)| {
                other != name
                    && text
                        .lines()
                        .map(str::trim)
                        // `foo.workspace = true` / `foo = { path = ... }`
                        .any(|l| {
                            l.starts_with(&format!("{name}.workspace"))
                                || l.starts_with(&format!("{name} ="))
                        })
            })
            .map(|(other, _)| other.as_str())
            .collect();

        assert!(
            dependents.is_empty(),
            "`{name}` emits a non-rlib output AND is depended on by {dependents:?}.\n\
             That combination is the CRUSH-16 hazard: its rlib has no filename hash, so\n\
             two build units of it overwrite each other at one path. Split the cdylib\n\
             into a leaf crate that depends on a plain-lib version of `{name}`."
        );
    }
}
