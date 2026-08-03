# CRUSH-55 — Exosphere in-tree crush divergence reconcile

| Field | Value |
|-------|-------|
| **ID** | CRUSH-55 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M9 — CROSS-REPO (exosphere) |

## Problem

Exosphere keeps its own `crush-lang` / `crush-cast`(1.0.0) / `casm` / `nanovm`
and calls crush-ast's walker binaries via SubprocessWalker. The drift is a
**both-ways feature divergence**, not an ancestor relationship: crush-ast has
the newer VM types (Bool/Map/Error/Bytes + s298 opcodes); exosphere has
corecaps stdlib, PolyglotContext sandboxing, AI-metadata, Wave3 gating.
A version bump cannot reconcile this — feature sets must merge.

## Approach

1. Delta inventory FIRST (read-only, both trees): per-crate feature matrix —
   what each side has that the other lacks (cite files). This artifact alone
   is worth the ticket.
2. Reconcile plan honoring the standing EXO-194 passive-convergence decision
   and exosphere-side ownership ([main]/buffy lane) — crush-ast side proposes,
   exosphere side disposes; nothing lands cross-tree unilaterally.
3. Execute in slices, each differentially tested; exo.* naming from CRUSH-48
   verified against exosphere's actual surface as part of slice 1.

## Definition of done

- [ ] Delta inventory committed (both repos cited)
- [ ] Reconcile plan agreed on the bridge with the exosphere lane owner
- [ ] First slice landed both sides or explicitly deferred with reasons

## Files in scope

- Inventory: read-only both trees; changes per-slice per-owner

## Gates

EXO-194 decision constraints; exosphere-lane coordination (or-delegation
signoff applies).
