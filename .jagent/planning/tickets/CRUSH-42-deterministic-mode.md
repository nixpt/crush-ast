# CRUSH-42 — Deterministic mode: ordered collections for state-touched structures

| Field | Value |
|-------|-------|
| **ID** | CRUSH-42 |
| **Priority** | P1 (also de-flakes CRUSH-77) |
| **Status** | Backlog |
| **Phase** | M7 |

## Problem

HashMap/HashSet iteration order is per-process-randomized and leaks into
execution: CRUSH-25's Resolution documents a still-live wrong-value-on-some-
layouts bug (cross-function THROW unwind depends on `program.functions`
iteration order), and it made a differential test flake 14/50 runs. No
`deterministic` cfg exists (s412 triage). Reproducibility is a prerequisite
for snapshot/replay (CRUSH-44) and stable differential testing (CRUSH-77).

## Approach

`deterministic` feature: BTreeMap/BTreeSet (or ordered variants) for
state-touched structures — program.functions, type registry, arena slot maps
(enumerate by audit, cite each). Type-alias indirection so the swap is one
line per site. CI job runs the suite under the feature; output must be
run-to-run identical (run twice, diff). Where order-dependence is a real BUG
(CRUSH-25 residue), determinism makes it reproducible — file, don't hide.

## Definition of done

- [ ] Audit list of state-touched collections committed
- [ ] `deterministic` feature builds + full suite green + twice-run identical
- [ ] CI lane added (mind CRUSH-CI-CACHE-1 warm-cache trap)
- [ ] CRUSH-25's residual order-dependent bug reproducible deterministically (filed)

## Files in scope

- `crates/crush-vm/src/*`, `crates/crush-frontend` type registry, Cargo features

## Gates

None. Gates CRUSH-44; de-flakes CRUSH-77.


## Dispatch

Imported from `workspace-meta/prompts/crush-backlog/CRUSH-42.txt` on 2026-08-24 so this tracked ticket contains the dispatch prompt metadata.

- Branch: `agent/horse/CRUSH-42`
- Repo: `crush-ast`
- Turns: `80`
- Runner: `claude`
- Scope: this ticket file is the canonical implementation spec; read it before changing code.
- Discipline: verify the repro against current `main` first, commit and push each meaningful unit, and update this ticket with the result.
- Verification: satisfy this ticket's Definition of done, include test evidence, and quote the real post-commit `HEAD` hash.
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs` unless this ticket explicitly scopes them; flag `crush_cast::Function`/`Program` shape changes before landing.
- Halt: stop and DM foreman if gates are unmet, scope is wrong, the repro no longer exists, sandbox blocks required work, or budget is nearly exhausted.
