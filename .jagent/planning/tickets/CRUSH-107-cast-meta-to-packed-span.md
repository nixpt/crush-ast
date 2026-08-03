# CRUSH-107 — CAST meta HashMap → packed Span + side table (contract-coordinated)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-107 |
| **Priority** | P1 (highest-leverage structural change in crush-cast, per audit) |
| **Status** | Backlog |
| **Phase** | Design/perf (CRUSH-71 audit finding #2) |

## Problem

Every `Statement`/`Expression` variant carries
`meta: HashMap<String, serde_json::Value>` (`crush-cast/src/lib.rs:80-405`)
that is essentially always empty: the parser writes `HashMap::new()` at 54
sites; exactly ONE site inserts anything (`parser/mod.rs:2183`). Cost: 48
bytes inline per node (~2× node size), inflated clones through the whole
pipeline, and serde_json welded into the core type graph.

## Approach

Replace with a packed `Span { lo: u32, hi: u32 }` per node + a side table for
the rare real metadata (the one `lang` insertion site).

**Design interlock — read before dispatch:**
- CRUSH-74 (span wiring) should implement its locations AS this Span, not by
  stamping the meta HashMap — if 74 hasn't started, land 107's type change
  first or together; if 74 already stamped meta, this ticket migrates it.
- ⚠ CONTRACT: the CAST shape is the nimbus (capsule VM) contract AND
  crush-visuals-source-bridge exhaustively matches these variants (VISUALS-8)
  — this change breaks both loudly. Coordinate: flag on the bridge before
  landing, sync with VISUALS-8's forward-compat policy, and check exosphere's
  fork divergence note (CRUSH-55) so the shapes don't drift further apart.

## Definition of done

- [ ] Node size measured before/after (≥ the audit's ~2× claim verified)
- [ ] serde_json out of crush-cast's core graph (serialization stays possible)
- [ ] The single real meta consumer (lang) migrated to the side table
- [ ] nimbus + visuals + notebook consumers compile green (coordinated PRs or
      one wave); `cargo test --workspace` green
- [ ] Clone-heavy pipeline paths re-benched (audit baseline)

## Files in scope

- `crates/crush-cast/src/lib.rs`, parser construction sites, compiler readers;
  client coordination per the interlock

## Gates

Bridge coordination with nimbus-lane + crush-visuals (VISUALS-8). Pairs with
CRUSH-74.
