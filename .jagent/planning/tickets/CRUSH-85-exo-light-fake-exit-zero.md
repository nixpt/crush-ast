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
