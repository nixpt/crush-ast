# CRUSH-63 — ML "GC policy brain" PoC (advisory-only)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-63 |
| **Priority** | P3 — aspirational, honestly labeled |
| **Status** | Backlog |
| **Phase** | M10 |

## Problem

ROADMAP M10's research item: a small on-device model observing per-program
allocation patterns and *advising* a heuristic choice between GC strategies.
Only meaningful once ≥2 real strategies exist (CRUSH-62) and instrumentation
exists to learn from. This is a PoC ticket: the deliverable is evidence
(does learned advice beat a static default on real workloads?), not
production integration.

## Approach

1. Instrument allocation/lifetime stats (cheap counters, off by default).
2. Offline: train a tiny model on corpus workloads; baseline = best static
   policy. Advisory hook only — the VM never blocks on inference.
3. Honest kill-criterion up front: if advice ≤ static default on the bench
   set, record the negative result via dejavue and close (trust the control,
   not the checkmark — a "working" brain that never beats static is a no).

## Definition of done

- [ ] Instrumentation (feature-gated) + workload dataset
- [ ] PoC comparison vs static baseline, result recorded either way
- [ ] Go/no-go decision written; production ticket filed only on go

## Files in scope

- `crates/crush-vm` (instrumentation), scratch training code (not shipped)

## Gates

CRUSH-62 (needs ≥2 strategies to choose between).
