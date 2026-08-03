# CRUSH-98 — crush-lang-wasm: walker→AOT path + AOT-C benchmark parity

| Field | Value |
|-------|-------|
| **ID** | CRUSH-98 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M11 |

## Problem

ROADMAP M11 says "migrate wasm_walker into a new crush-lang-wasm crate" — but
`crates/crush-lang-wasm` ALREADY EXISTS (created in the crush-lang-* family
unification 9a9f4a6, s412 triage), so the ticket is HALF-DONE at filing.
Verify what the crate contains vs the old `wasm_walker` (both may exist —
repo-root TASKS.md says wasm_walker was "verified with WASI integration
tests"); consolidate if duplicated. Real remaining scope: the walker→AOT path
+ benchmark parity within 2× of AOT-C on nqueens/sieve/mergesort.

## Approach

1. Crate consolidation audit (wasm_walker vs crush-lang-wasm — one survives).
2. AOT path via CRUSH-103's shared plumbing; .wat/.wasm fixtures with WASI
   calls lowering to VM capabilities (io.print precedent exists).
3. Bench trio vs AOT-C; 2× parity target measured + quoted honestly (a miss
   is a finding, not a fudge).

## Definition of done

- [ ] One wasm walker crate (other retired with note)
- [ ] wasm fixture → native via AOT, differential-green
- [ ] Bench table committed (parity met or gap explained)

## Files in scope

- `crates/crush-lang-wasm`, `crates/wasm_walker` (retirement), bench fixtures

## Gates

CRUSH-103; M8 wasm32 lane (CRUSH-49/50) for the wasm32-target side.
