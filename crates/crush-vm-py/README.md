# crush-vm-py

PyO3 bindings for the canonical CVM1 VM — this crate builds the `crush_vm`
Python wheel (`import crush_vm; crush_vm.run_blob(...)`).

## Building the wheel

```bash
cd crates/crush-vm-py
maturin build --release        # features/no-default-features come from pyproject.toml
```

## Testing the wheel

The crate has no Rust tests (it is a `cdylib` and is not part of the Rust
workspace, so `cargo test` never reaches it). There is a Python smoke test
that runs against an installed wheel:

```bash
maturin build --release
pip install target/wheels/crush_vm-*.whl
python crates/crush-vm-py/tests/test_wheel.py
```

It needs no pytest — the file is a plain script with `test_*` functions and a
`__main__` runner, so it also works under pytest if the repo adopts it. The
fixtures in `tests/fixtures/` are real CVM1 blobs assembled by crush-ast's
own assembler (`crush_vm::assemble(...).to_blob()`), not mocks.

**CI does not run this yet** — no workflow job builds the wheel. Tracked as
[CRUSH-275](../.jagent/planning/tickets/CRUSH-275-wheel-ci-smoke-test.md).
Until then, run it by hand after any change to `src/lib.rs` or to the
`crush-vm` API it binds.

This crate is **not a member of the crush-ast workspace** — it is an
independent resolution root (`exclude` in the root `Cargo.toml`, plus its own
empty `[workspace]` table), the same treatment `crush-web` gets. So
`cargo test --workspace` at the repo root does not build it, and needs no
libpython. Build it from this directory.

> **Stable ABI (abi3).** The wheel is built against the stable ABI
> (`abi3` + `abi3-py311`, the same pair `crush-python` uses), so one wheel
> serves every interpreter from 3.11 up. This also lifts pyo3 0.23's
> interpreter ceiling: without abi3, pyo3 refuses to build on a Python 3.14
> interpreter (`configured Python interpreter version (3.14) is newer than
> PyO3's maximum supported version (3.13)`). That limitation was inherited
> from when the bindings lived in `crush-vm` behind the `python` feature — see
> `.jagent/planning/archive/TASKS-2026-10-14.md`. The wheel's *behaviour* is
> unchanged by the ABI switch; only the set of interpreters it loads on is.

## What `run_blob` returns

A dict with:

| key | meaning |
|---|---|
| `output` | everything the program printed |
| `result` | the top of the final value stack (`None` if the stack is empty) |
| `return_value` | the entry function's explicit `return` value, or `None` |
| `stack` | the final value stack |
| `steps` | steps executed |
| `halted` | whether the program halted |

`result` and `return_value` are **not** the same thing. A program that
returns a value leaves an *empty* stack and puts the value in
`return_value`, so reading only `result` reports `None` for exactly the
programs whose value matters. This matches the Rust `VmResult` and the
`crush-lang-sdk` `VmResult`.

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
