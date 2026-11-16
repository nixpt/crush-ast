# CRUSH-479 — crush.toml / capsule manifest gets a format-version field; lockfile version is enforced

| Field | Value |
|-------|-------|
| **ID** | CRUSH-479 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#479](https://github.com/nixpt/crush-ast/issues/479) |
| **Size** | S |
| **Dependencies** | none |

## Problem

Manifest (crates/crush-pkg/src/manifest.rs:24-40) has capsule.version (the package version) but no manifest-format version, so old and new manifests cannot be told apart after the freeze (RC F8, T3). Manifest::from_str (manifest.rs:455+) migrates legacy shapes ad hoc by key sniffing. The lockfile struct has `version: u32` and write_lock writes 1 (gitdep.rs:63-68,92-95), but read_lock ignores it (gitdep.rs:71-80), so a future lockfile v2 would be silently misread. Ruling 14: add a format-version field before freeze.

## Evidence (at origin/main 7ecd2515)

- crates/crush-pkg/src/manifest.rs:24-40,455-475,792
- crates/crush-pkg/src/gitdep.rs:63-95
- docs/planning/RELEASE-1.0-CRITERIA.md:98,135
- ruling 14

## Goal

An optional top-level `format-version` (integer, default 1 when absent) on Manifest. from_str rejects a value above the supported version with an actionable error naming the supported version. The scaffold template (manifest.rs:792) emits format-version = 1. read_lock rejects a lock version greater than 1 (0/absent = legacy, accepted). The rule (additive = same version, breaking = bump) is documented in docs/FILE-FORMATS.md.

## Non-goals

- Runtime pins, lockfile redesign, `crush env` (ruling 14: later; #359)
- Changing capsule.version semantics
- Renaming capsule.toml/crush.toml discovery

## Files

- crates/crush-pkg/src/manifest.rs
- crates/crush-pkg/src/gitdep.rs
- docs/FILE-FORMATS.md

## Done when

- cargo test -p crush-pkg: a manifest with no field loads; format-version = 1 loads; = 2 fails naming the supported version; a lockfile with version = 2 fails; the scaffold template parses and contains format-version = 1
- existing crush-pkg tests pass unchanged
- cargo +1.100.0 clippy -p crush-pkg --all-targets -- -D warnings clean

## Depends on

- none

## Refs

- ruling 14
- RC gates F8, T3
- #359 (later, out of scope)

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
