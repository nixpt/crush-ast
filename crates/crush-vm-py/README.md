# crush-vm-py

PyO3 bindings for the canonical CVM1 VM — this crate builds the `crush_vm`
Python wheel (`import crush_vm; crush_vm.run_blob(...)`).

## Building the wheel

```bash
cd crates/crush-vm-py
maturin build --release        # features/no-default-features come from pyproject.toml
```

This crate is **not a member of the crush-ast workspace** — it is an
independent resolution root (`exclude` in the root `Cargo.toml`, plus its own
empty `[workspace]` table), the same treatment `crush-web` gets. So
`cargo test --workspace` at the repo root does not build it, and needs no
libpython. Build it from this directory.

> **Pre-existing limitation:** pyo3 0.23 supports Python ≤ 3.13. On a box whose
> default interpreter is newer, the build fails in pyo3's own build script
> (`configured Python interpreter version (3.14) is newer than PyO3's maximum
> supported version (3.13)`). This is inherited unchanged from when the
> bindings lived in `crush-vm` behind the `python` feature — see
> `.jagent/planning/TASKS.md`. Adopting `abi3` (as `crush-python` does) would
> fix it, but changes the wheel's ABI contract and is the bridge owner's call.

## Why this is a separate crate

These bindings used to live in `crush-vm/src/python.rs` behind a `python`
feature, which required `crush-vm` to declare `crate-type = ["lib", "cdylib"]`.

Cargo drops the `-C extra-filename=-<hash>` suffix for any target whose
crate-type includes a non-rlib output, so `crush-vm`'s rlib was written to a
single fixed path (`target/debug/deps/libcrush_vm.rlib`). That is fine for a
leaf cdylib and corrupting for a crate that is *also* a normal rlib
dependency of 100 other build units: the workspace legitimately builds two
`crush-vm` units — one in the target graph, one in the host graph (pulled in
by `crush-macros`, a proc-macro crate whose tests depend on `crush-vm`) — and
both wrote that same path. Last writer won. When the host-graph unit won,
`crush-lang-sdk` linked a `crush_vm` built against the host `casm` while its
`crush_frontend` was built against the target `casm`:

```
error[E0308]: mismatched types
   --> crates/crush-lang-sdk/src/differential.rs:200:45
    |
200 |     let fastvm = match crush_vm::run_fastvm(&casm) {
    |                        -------------------- ^^^^^ expected `casm::Program`,
    |                                                    found a different `casm::Program`
```

Being a race, it was order-dependent: `cargo check --workspace` passed,
`cargo test -p <crate>` passed, and `cargo test --workspace` failed
intermittently — sometimes as the E0308 above, sometimes as
`E0463: can't find crate for crush_lang_sdk`.

`CRUSH-16` diagnosed this and set `crush-vm` back to `crate-type = ["lib"]`.
The PyO3 wheel commit (`4137646`) re-added the `cdylib` and silently
regressed it, because no CI job ran `cargo test --workspace`. This crate
restores the invariant — the same split already used by `crush-vm-capi` (C
ABI) and `crush-python` (crush-cast bindings) — and CI's `Test (workspace)`
job keeps it from regressing silently a third time.

**Rule of thumb:** a crate that is depended on as a normal Rust library must
stay `crate-type = ["lib"]`. Put every `cdylib`/`staticlib` in its own leaf
crate.
