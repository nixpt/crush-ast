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


## Dispatch

Imported from `workspace-meta/prompts/crush-backlog/CRUSH-63.txt` on 2026-08-24 so this tracked ticket contains the dispatch prompt metadata.

- Branch: `agent/horse/CRUSH-63`
- Repo: `crush-ast`
- Turns: `60`
- Runner: `claude`
- Scope: this ticket file is the canonical implementation spec; read it before changing code.
- Discipline: verify the repro against current `main` first, commit and push each meaningful unit, and update this ticket with the result.
- Verification: satisfy this ticket's Definition of done, include test evidence, and quote the real post-commit `HEAD` hash.
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs` unless this ticket explicitly scopes them; flag `crush_cast::Function`/`Program` shape changes before landing.
- Halt: stop and DM foreman if gates are unmet, scope is wrong, the repro no longer exists, sandbox blocks required work, or budget is nearly exhausted.
