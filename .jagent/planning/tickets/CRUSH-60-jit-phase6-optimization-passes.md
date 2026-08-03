# CRUSH-60 — JIT Phase 6: optimization passes (const fold, DCE, small-fn inlining)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-60 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M10 |

## Problem

No optimization passes exist in crush-jit (verify: src is compiler.rs /
runtime.rs / value.rs / lib.rs). ⚠ NAMING TRAP (s412 triage): the GVN
references at crush-jit/src/lib.rs:2371/2448/2452 are notes about an UNSOLVED
Cranelift-hoisting workaround from the ex-CRUSH-17 work ("7 approaches
attempted, none have defeated Cranelift's hoisting of load(call_stack_top)
across re-entrant blocks") — they are not an existing pass, and any Phase-6
work touching block layout must not regress that carefully-balanced
workaround (CRUSH-87's residue).

## Approach

Prefer enabling/configuring Cranelift's own opt level + shaping CLIF to be
optimizable over hand-writing passes; add crush-level passes only where
measurement shows Cranelift can't (const-fold across the tag encoding,
small-fn inlining at the FastOp level). Every pass gated by differential
fixtures (no semantic drift) + bench evidence (no bench win → no merge).

## Definition of done

- [ ] Per-pass: differential-green + quoted bench delta on named workloads
- [ ] CRUSH-87's hoisting workaround still holds (its regression test green)
- [ ] Passes documented in docs/design (what runs when, how to disable)

## Files in scope

- `crates/crush-jit`

## Gates

CRUSH-59. Gates CRUSH-61.
