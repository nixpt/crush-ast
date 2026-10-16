# CRUSH-30a — Exhaustive-match lint residue: per-type narrowing, missing_arms, workspace tracking

| Field | Value |
|-------|-------|
| **ID** | CRUSH-30a (follow-up to CRUSH-30, closed 2026-10-16) |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M5 |
| **Dependencies** | type inference landing (unblocks `type_name`); CRUSH-28 crush-index for the variant registry |

## Origin

Minted 2026-10-16 from CRUSH-30's close-out ruling (buffy, branch
`agent/buffy/CRUSH-quickwins-11-35`). CRUSH-30's annotation-gated wildcard
lint (`E-EXH-001`, `crush-frontend/src/exhaustive_check.rs`) is accepted as
the ticket's deliverable; this ticket carries the explicitly-deferred v1 gaps.

## Scope

1. **Per-type narrowing**: resolve each `ExhaustiveMatchSite.type_name`
   (currently `String::new()` by design — cast_enrich.rs:161) so
   `check_exhaustiveness` warns only on matches over tracked types instead
   of any wildcard match in the module.
2. **Variant registry + `missing_arms`**: populate `missing_arms` from a
   known-variants table (CRUSH-28 index ingest or a CASM side artifact),
   enabling the "add variant without new match site → warn" signal.
3. **Workspace tracking**: cross-file match-site accumulation + optional
   `.exhaustive.json` side artifact next to CASM (per CRUSH-30's original
   technical approach §1).
4. **Diagnostic id**: consider migrating `E-EXH-001` → `W-EXM01` per the
   original spec (breaking-change check against existing consumers first).

## Non-goals

- Sound exhaustiveness guarantees (stays best-effort, per CRUSH-30 v1).
- IDE integration.

## Files likely in scope

- `crates/crush-frontend/src/exhaustive_check.rs`
- `crates/crush-frontend/src/cast_enrich.rs`
- `crates/crush-index` (variant registry ingest, coordinate with CRUSH-28)

## Dispatch

- Branch: `agent/<name>/CRUSH-30a`
- Verify the E-EXH-001 baseline still fires (exhaustive_check tests) before
  narrowing; each scope item lands incrementally with its own test.
