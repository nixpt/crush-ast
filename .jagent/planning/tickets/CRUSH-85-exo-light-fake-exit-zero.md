# CRUSH-85 — exo-light: fabric_executor fakes exit_code:0 when crush-run is missing

| Field | Value |
|-------|-------|
| **ID** | CRUSH-85 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | Correctness spine (s412) — CROSS-REPO |
| **Repo** | `openko-network/openko` `runtime/exo-light` (ticket anchored here; work dispatches there) |

## Problem

Panini client-survey capture (2026-08-02): exo-light's `fabric_executor` falls
back to a fabricated `exit_code: 0` success when no `crush-run` binary is
found (verify in `/home/nixp/WORKSPACE/projects/openko-network/openko`
`runtime/exo-light`). A binary rename, PATH change, or packaging slip turns
every fabric execution into a silent no-op success — the CI-green-because-
nothing-ran failure class. Same family as CRUSH-84/72: fabricating success
for an input (environment) the code doesn't actually handle.

## Approach

Missing binary → hard error (distinct exit/error variant naming the binary and
searched paths). If a degraded no-op mode is genuinely wanted somewhere, make
it opt-in and loudly logged — never the fallback. Add a test with an empty
PATH/temp dir asserting the error (not success).

## Definition of done

- [ ] Missing crush-run → explicit error, test-asserted
- [ ] No code path fabricates exit 0 without executing
- [ ] openko test suite green

## Files in scope

- `runtime/exo-light` fabric executor (openko repo — branch there)

## Gates

None.


## Dispatch

Imported from `workspace-meta/prompts/crush-backlog/CRUSH-85.txt` on 2026-08-24 so this tracked ticket contains the dispatch prompt metadata.

- Branch: `agent/horse/CRUSH-85`
- Repo: `openko`
- Turns: `40`
- Runner: `claude`
- Scope: this ticket file is the canonical implementation spec; read it before changing code.
- Discipline: verify the repro against current `main` first, commit and push each meaningful unit, and update this ticket with the result.
- Verification: satisfy this ticket's Definition of done, include test evidence, and quote the real post-commit `HEAD` hash.
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs` unless this ticket explicitly scopes them; flag `crush_cast::Function`/`Program` shape changes before landing.
- Halt: stop and DM foreman if gates are unmet, scope is wrong, the repro no longer exists, sandbox blocks required work, or budget is nearly exhausted.
