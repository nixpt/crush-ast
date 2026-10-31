# CRUSH-zorro-53 — VEC_ADD/VEC_DOT/MAT_MUL silently coerce non-numbers to 0.0 and truncate mismatched lengths

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-53 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | VM |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude (vega) — 2026-10-15, found writing CRUSH-zorro-52; split out per foreman review |

## Problem

The VM's vector built-ins (crush-vm `scheduler.rs` VEC_ADD / VEC_DOT / MAT_MUL ≈815-900, and the
portable VM ≈811-900) silently produce wrong numbers:

- an element that isn't a number becomes `0.0` (`_ => 0.0`);
- arrays of different lengths are truncated to the shorter one (`len = min(a, b)`).

Reproduced on main (`d54244a`) through the crush-vm library with CVM1 assembly:

```text
[1, 2, 3] VEC_ADD ["x", 10]     → [1.0, 12.0]   ("x" → 0, third element dropped)
[1, 2]    VEC_DOT [5, "y", 7]   → 5.0           ("y" → 0, 7 dropped)
```

**Reachability:** today no frontend emits these ops — no parser or walker produces CAST
`VectorMath`, and `casm_to_vm` has no lowering for them — so it is reachable from hand-written
CVM1 assembly / bytecode only. Hence P1, not P0; it becomes P0 the moment a frontend lowers to them
(and CRUSH-zorro-52's GPU offload would otherwise copy the behaviour).

## Fix

Both engines: a non-numeric element is a type error naming the index and the type; mismatched
lengths are an error (`VEC_ADD`/`VEC_DOT`: lengths must match; `MAT_MUL`: inner dimensions must
match, rows must be equal-length arrays). Ints stay exact where both operands are ints, or document
the f64 widening.

## Done when

- [ ] Both repros above are errors on the scheduler and the portable VM (a `run_both`-style test),
      plus MAT_MUL ragged/mismatched-dimension cases.
