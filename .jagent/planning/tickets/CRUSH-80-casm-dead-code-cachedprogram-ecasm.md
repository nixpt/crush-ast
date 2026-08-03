# CRUSH-80 — casm dead code: CachedProgram + ecasm.rs — wire or delete

| Field | Value |
|-------|-------|
| **ID** | CRUSH-80 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | Hygiene (s412) |

## Problem

Panini capture (2026-08-02): `casm` carries `CachedProgram`/`to_cached`
(lib.rs:246-610) — doc-promises 10-100x speedup, is wired to nothing, and is
O(F²) as written — plus `ecasm.rs` (1039 lines, zero external references).
Dead code that advertises a capability misleads both agents and humans
(the "both ends built, middle missing" pattern in miniature). Re-verify the
zero-references claim with a workspace-wide grep at dispatch.

## Approach

Decide per artifact: wire it (only if CRUSH-83's compile-cache design actually
wants it — likely not, given O(F²)) or delete it with a dejavue note. Deletion
is the default; CRUSH-83 owns the real caching design.

## Definition of done

- [ ] CachedProgram + ecasm.rs either deleted (with dejavue entry) or wired
      with a consumer + test — no third state
- [ ] `cargo test --workspace` green; no orphaned pub API remains

## Files in scope

- `crates/casm/src/lib.rs`, `crates/casm/src/ecasm.rs`

## Gates

Read CRUSH-83's direction first (subsume-or-delete question).
