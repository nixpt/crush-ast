# CRUSH-59 — crush-jit FastOps audit closure: every op compiles or is refused-and-covered

| Field | Value |
|-------|-------|
| **ID** | CRUSH-59 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M10 |

## Problem

The JIT implements a subset of the FastOp set (July count: ~31 of ~86 — STALE:
M2 Phase 2–4 landings since then; count the current match arms in
crush-jit/src/compiler.rs vs the enum in crush-vm/src/fastvm/instructions.rs
as this ticket's first commit). CRUSH-72 makes the gap SAFE (refuse + fall
back); this ticket makes it SMALL: implement the remaining ops, each verified
under the differential harness.

## Approach

Audit table (op → compiled/refused/na) committed first; then implement in
coverage-ordered batches (string ops, array/map, capability calls — whatever
real corpus programs hit most, per CRUSH-71's bench baseline); each batch
lands with differential fixtures (CRUSH-77) proving JIT ≡ interpreter. Some
ops may be legitimately permanent-refuse (e.g. EXEC_LANG) — document, don't
force.

## Definition of done

- [ ] Audit table committed + kept current in the ticket
- [ ] Every FastOp: compiled+differentially-verified, or documented
      permanent-refuse (fallback covered by CRUSH-72's test)
- [ ] Bench delta on corpus programs quoted (the point of a JIT)

## Files in scope

- `crates/crush-jit/src/compiler.rs` (+ runtime.rs/value.rs as ops need)

## Gates

CRUSH-72 (bail-out), CRUSH-77 (differential harness). Gates CRUSH-60.
