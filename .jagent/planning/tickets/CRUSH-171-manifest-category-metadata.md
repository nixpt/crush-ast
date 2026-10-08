# CRUSH-171 — Package manifest category / platform metadata

| Field | Value |
|-------|-------|
| **ID** | CRUSH-171 |
| **Priority** | P4 |
| **Status** | Done (PR open) |
| **Phase** | M4 |
| **Assignee** | panini-d |
| **Dependencies** | none |
| **Estimated effort** | S (~25 turns) |
| **Relay lane** | D4 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

`crush-pkg`'s `Manifest` has no category or target-platform fields; crush-capsules' `CAPSULE_CATEGORIES.md` / `hub.json` sketched a scheme for cataloguing capsules.

## Success criteria

- [x] optional `category` and `platforms` fields in the manifest (serde-default, old manifests still load)
- [x] `crush-pkg show` prints them; validation of known values with a clear error
- [x] schema documented

## Technical approach

- Additive manifest change.

## Files to modify

- `crates/crush-pkg/src/manifest.rs`
- docs

## Non-goals

- a registry / hub service

## Source (reference only — re-implement, don't copy)

- `nixpt/crush-capsules` `CAPSULE_CATEGORIES.md`, `hub.json`

## Evidence (panini-d, 2026-10-07, commit `4d9e03b`)

- `CapsuleSection.category: Option<String>` and `platforms: Vec<String>`,
  both serde-default and skipped when empty. Known values live in
  `crush_pkg::manifest::{CATEGORIES, PLATFORMS}`:
  - categories: `cli library app service game dev-tool language example`;
  - platforms: `linux macos windows web`, where `web` is crush-web's wasm VM.
- Validation runs in `Manifest::validate` (called by `from_str`):
  - an unknown category or platform is an error that lists the accepted values;
  - a duplicate platform is an error;
  - `web` on a Script/Native capsule is an error, because the browser only runs Crush.
- The old crush-capsules sketch's names (`development-tool`, `user-app`, …)
  are migrated on load. Its top-level `[platform]` table is ignored and
  doesn't collide.
- The scheme is re-implemented from the sketch's idea, not copied: different
  category set, and a list where the sketch had booleans.
- `crush-pkg show` prints both fields. An unknown value fails with
  `crush-pkg[E-MANIFEST]: unknown [capsule] category "toolz"; expected one of: …`.
- Schema: `crates/crush-pkg/MANIFEST.md`. No manifest doc existed before;
  this one also covers the basic `[capsule]` fields.
- Client impact: `CapsuleSection` gained two public fields. No crate outside
  crush-pkg builds it with a literal: checked the workspace and the peer
  projects (`surfer-browser` only names crush-pkg in a comment). crush-pkg is
  unpublished. The 3 in-crate literals were updated.
- Tests: 8 manifest unit tests (optional, round-trip, every known value,
  unknown category, unknown/duplicate platform, web Crush-only, legacy
  migration, `[platform]` table ignored) + 1 integration test against the
  binary (`show` prints the fields; an unknown category is refused). `cargo
  test -p crush-pkg` green, and clippy reports nothing in new code.
- Not done: checking `platforms` against the capabilities a program uses.
  That belongs with CRUSH-170 (capability inference).
