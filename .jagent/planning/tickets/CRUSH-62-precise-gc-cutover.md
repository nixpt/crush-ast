# CRUSH-62 — Conservative → precise GC cutover (per CRUSH-78's decision)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-62 |
| **Priority** | P2 |
| **Status** | Backlog — HARD-GATED on CRUSH-78 |
| **Phase** | M10 |

## Problem

ROADMAP M10 proposes shadow-stack → real stack-map precise GC to eliminate
pauses for long-lived programs. But s412 triage found zero GC code in crates/
at all — there may be nothing to "cut over" FROM. This ticket is intentionally
thin until CRUSH-78 (memory-model decision) lands: its outcome decides whether
this becomes "implement precise GC on the chosen model", "wire arena epochs",
or "closed — decision accepted cycle-leaks".

## Approach

Written after CRUSH-78. Re-spec this file at that point (keep the ID; replace
this body). Whatever lands must carry: pause-time benchmarks on long-lived
workloads, differential-green, JIT stack-map story if precise.

## Definition of done

- [ ] Re-specified post-CRUSH-78 with concrete design
- [ ] (then) implementation per that spec with pause + correctness evidence

## Files in scope

- TBD by CRUSH-78 (crush-vm value model core)

## Gates

CRUSH-78 (hard). Gates CRUSH-63.


## Dispatch

Imported from `workspace-meta/prompts/crush-backlog/CRUSH-62.txt` on 2026-08-24 so this tracked ticket contains the dispatch prompt metadata.

- Branch: `agent/horse/CRUSH-62`
- Repo: `crush-ast`
- Turns: `30`
- Runner: `claude`
- Scope: this ticket file is the canonical implementation spec; read it before changing code.
- Discipline: verify the repro against current `main` first, commit and push each meaningful unit, and update this ticket with the result.
- Verification: satisfy this ticket's Definition of done, include test evidence, and quote the real post-commit `HEAD` hash.
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs` unless this ticket explicitly scopes them; flag `crush_cast::Function`/`Program` shape changes before landing.
- Halt: stop and DM foreman if gates are unmet, scope is wrong, the repro no longer exists, sandbox blocks required work, or budget is nearly exhausted.
