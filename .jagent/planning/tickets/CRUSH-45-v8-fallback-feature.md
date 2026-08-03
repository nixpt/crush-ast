# CRUSH-45 — V8 fallback lane for dynamic JS (feature-gated)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-45 |
| **Priority** | P3 (heavy dep — spec gate before any code) |
| **Status** | Backlog |
| **Phase** | M7 |

## Problem

The swc-lowered JS path covers a JS subset; genuinely dynamic JS (eval,
proxies, prototype tricks) can't lower to CAST. ROADMAP proposes a
feature-gated V8 fallback (`v8-fallback`, snapshot-based, DevTools-attachable)
so dynamic JS runs authentic while remaining opt-in.

## Approach

1. FIRST deliverable: build-weight + isolation analysis — the `v8` crate's
   compile cost/platform matrix, and proof the feature stays fully out of
   default builds (workspace `--no-default-features` and default builds show
   zero v8 in the dep graph). If the cost analysis says no, this ticket
   closes as REJECTED with the analysis as the artifact — that is a valid
   outcome.
2. If go: `v8-fallback` feature in the JS lane; route = try lower-to-CAST,
   else V8 execute with capability-mediated host surface (no raw Node APIs);
   result marshaling through the existing polyglot value path (CRUSH-68's
   typed marshaling).
3. Divergence policy: V8-lane results are authoritative-for-JS; document the
   seam (sangam's opt-in-divergence doctrine, not faked green).

## Definition of done

- [ ] Cost/isolation analysis committed (go / no-go recorded via dejavue decision)
- [ ] If go: dynamic-JS fixture runs under the feature, default builds untouched
- [ ] CI: feature-gated lane only; default lanes unaffected

## Files in scope

- `crates/crush-lang-js`, Cargo features, docs/design

## Gates

Cost analysis is the gate. Off-by-default forever.
