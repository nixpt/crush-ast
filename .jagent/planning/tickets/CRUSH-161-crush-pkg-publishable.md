# CRUSH-161 — Make `crush-pkg` publishable (unblocks squeeze)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-161 |
| **Priority** | P2 |
| **Status** | Done (PR open; publish is foreman + captain) |
| **Phase** | M0 |
| **Assignee** | panini-d |
| **Dependencies** | none |
| **Estimated effort** | S (~20 turns) |
| **Relay lane** | D1 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

`crush-pkg` is the only crate squeeze needs that is not on crates.io (checked 2026-10-07; its deps are all published at 0.3.9, `crush-buckets` 0.1.0). It is simply absent from CRUSH-104's publish set.

## Success criteria

- [x] `cargo publish --dry-run -p crush-pkg` passes (readme path, `buckets` dep carries `version`, every internal dep has `path` + `version`)
- [x] `crush-pkg` added to the publish lane / `crates-publish-sync` order
- [ ] the actual `cargo publish` is left to foreman (irreversible) — this ticket ends at a green dry-run + a DM

## Technical approach

- Fix whatever the dry-run reports; no API changes.

## Files to modify

- `crates/crush-pkg/Cargo.toml`
- publish-lane config

## Non-goals

- publishing (foreman's action)
- renaming the crate

## Source (reference only — re-implement, don't copy)

- `nixpt/squeeze` RELEASE.md / STATE.md

## Evidence (panini-d, 2026-10-07, commit `19368b4`)

- Readme path and the `buckets` dep's `version` were already fine
  (`README.md` lands in the tarball; `crush-buckets` carries `version = "0.1.0"`).
- The one real blocker: `E0560 SandboxProfile has no field named
  extra_rw_binds / net_ns` when the tarball was compiled against crates.io
  `crush-buckets` 0.1.0. The buckets checkout grew those fields without a
  version bump. Fixed in `runners.rs` with `..Default::default()` (the shape
  `crush-vm/src/bucket_exec.rs` already used). Captured the buckets-side
  version skew in TASKS.md.
- After the fix: `cargo publish --dry-run --allow-dirty -p crush-pkg` →
  exit 0, `Packaged 26 files`, `Verifying crush-pkg v0.3.9 … Finished`,
  `aborting upload due to dry run`. **No `[patch.crates-io]`**: every dep is
  live (crush-vm/cast/casm/frontend/lang-sdk/diagnostics 0.3.9, crush-buckets 0.1.0).
- `cargo test -p crush-pkg`: all green (65 + 19 + 8 + 3 + 2 passed, 2 ignored).
- Publish lane: `CRUSH-104-publish-lane.md` gains `-p crush-pkg` in the
  pre-publish package step, `cargo publish -p crush-pkg` last in the order,
  and an addendum. The multi-package `cargo package` line with `-p crush-pkg`
  added was not re-run here (the crates.io dry-run above is the stronger
  check); re-run it on the release tag as step 1 says.
- Not done here: the publish (C-7, foreman + captain).
