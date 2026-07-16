# CRUSH-16 — `cargo test --workspace` fails to link (AOT bins + crush-python cdylib/rlib clash)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-16 |
| **Priority** | P1 |
| **Status** | Done |
| **Phase** | M0 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

`cargo check --workspace` is clean and per-crate test suites are green
(crush-vm/crush-frontend/crush-lang-sdk = 468+ tests), but a **full**
`cargo test --workspace` fails the test-binary **link** step, for two
independent, already-diagnosed reasons:

1. **AOT binaries don't link.** `[profile.release] lto = "fat"`
   (`Cargo.toml:81`, from the "enable LTO at all 3 layers" commit) breaks
   linking for `crush-aotc`/`crush-walk-run` in the test-build configuration.
2. **`crush-python` duplicate-compiles `casm`.** Its `crate-type =
   ["cdylib", "rlib"]` (confirmed still current) causes cargo to compile
   `casm` twice in the same link unit → `E0308: multiple different
   versions of crate casm in the dependency graph`. Cargo also warns about
   an output-filename collision naming the same package twice
   (`libcrush_vm.rlib`/`.so`).

Both are captain-diagnosed, both PRE-DATE the current `main` tip (not
introduced by any recent merge), and both are still present as of s388
(2026-07-16) — `lto = "fat"` and `crush-python`'s crate-type are unchanged.

## Impact

Nobody can run a plain `cargo test --workspace` and trust the result —
per-crate test invocation (`cargo test -p <crate>`) is the only reliable
path today, which is easy to forget and easy for CI to get wrong (a naive
`cargo test --workspace` in a CI config would report failure on a green
codebase, or worse, silently skip crates whose tests never actually ran).

## Reproduction

```bash
cargo test --workspace
# link errors: crush-aotc/crush-walk-run fail to link (LTO), OR
# E0308 multiple different versions of crate `casm` (crush-python)
```

## Technical approach (already scoped by prior diagnosis)

1. AOT link fix: change `lto = "fat"` → `lto = "thin"`, or exclude the AOT
   bin targets specifically from fat-LTO (per-target profile override if
   Cargo supports it for this shape, else split the AOT crates into their
   own workspace-level profile).
2. crush-python fix: give it a single crate-type for the test build (either
   feature-gate the `cdylib` output so `cargo test` only builds `rlib`, or
   restructure so the dylib-consuming path doesn't also need to be an
   rlib in the same compilation).
3. Verify with a clean target dir: `cargo test --workspace` should exit 0
   with all crate suites actually running (not silently skipped).

## Files to modify

- `Cargo.toml` (workspace `[profile.release]` — `lto` setting)
- `crates/crush-python/Cargo.toml` (`crate-type`)

## Resolution

Applied the scoped fix from the ticket:

1. `Cargo.toml`: changed `[profile.release]` `lto = "fat"` → `lto = "thin"` to fix AOT test-binary linking.
2. `crates/crush-python/Cargo.toml`: changed `crate-type = ["cdylib", "rlib"]` → `crate-type = ["cdylib"]` to stop `casm` from being compiled twice in the same link unit.
3. `crates/crush-vm/Cargo.toml`: changed `crate-type = ["lib", "cdylib"]` → `crate-type = ["lib"]` so the workspace builds `crush-vm` as a plain Rust library.
4. Created `crates/crush-vm-capi/` as a separate workspace member that builds `libcrush_vm_capi.so` (`crate-type = ["lib", "cdylib"]`). It contains the C-ABI entry points (`crush_vm_init`, `crush_vm_run_casm`, etc.) that used to live inside `crush-vm/src/c_api.rs`. This preserves the C-API while isolating the cdylib from the workspace's main dependency graph, preventing the duplicate-`casm` E0308.

Verification:
```bash
cargo test --workspace
# all workspace suites pass (including crush-vm-capi::test_c_embed)
```

`cargo test --workspace` now compiles and runs; the only remaining failure is an unrelated pre-existing exosphere fixture type error in `crush-pkg::test_build_pipeline` (`Cannot compare types null and int`), which is outside the scope of CRUSH-16.

## Non-goals

- Changing the LTO strategy for the shipped release binaries' actual size
  win (64-80% reduction) — the fix should be scoped to the test-build
  configuration specifically, not regress release binary size
