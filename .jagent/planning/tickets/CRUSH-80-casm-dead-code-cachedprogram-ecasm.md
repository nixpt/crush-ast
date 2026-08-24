# CRUSH-80 — casm dead code: CachedProgram + ecasm.rs — wire or delete

| Field | Value |
|-------|-------|
| **ID** | CRUSH-80 |
| **Priority** | P2 |
| **Status** | Done — deleted in `1cd2506` / v0.3.6 |
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

- [x] CachedProgram + ecasm.rs either deleted (with dejavue entry) or wired
      with a consumer + test — no third state
- [x] No orphaned public `CachedProgram`/`to_cached` API remains

## Resolution

Resolved by deletion in `1cd2506` (`casm: delete dead ecasm.rs + CachedProgram
remnants (CRUSH-80)`), released in `v0.3.6`.

The stale `crates/casm/src/ecasm.rs` file was removed, and the orphaned
`CachedProgram`/`to_cached` remnants were removed from `crates/casm/src/lib.rs`.
CRUSH-83 remains the correct home for any future content-hash compile cache or
incremental-unit design.

Verification:

```text
CARGO_TARGET_DIR=/tmp/target-crush-ast-foreman-review CARGO_BUILD_JOBS=2 cargo check -p crush-vm -p crush-lang-sdk -p crush-frontend -p crush-aot -p crush-aotc
passed
```

## Files in scope

- `crates/casm/src/lib.rs`, `crates/casm/src/ecasm.rs`

## Gates

Settled by deletion. CRUSH-83 is no longer blocked on deciding whether to reuse
this code.
