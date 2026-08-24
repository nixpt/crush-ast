# CRUSH-102 — Notebook cross-language state-sharing CI tests

| Field | Value |
|-------|-------|
| **ID** | CRUSH-102 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M11 — CROSS-REPO (crush-workspace/crush-notebook) |

## Problem

The "Jupyter-killer" claim (cells of different languages sharing state) must
be CI-verifiable or it's marketing. CRUSH-101 builds the mechanism; this
ticket pins it with an integration suite that runs in the notebook's CI on
every push.

## Approach

Scripted-kernel integration tests (no UI): notebook-as-fixture files
exercising state sharing matrices (define-in-A/read-in-B across the
in-process language set), mutation visibility ordering, error isolation (a
failing cell doesn't corrupt the session), and session restart semantics.
Assert on outputs the way CRUSH-73 does — expected-output fixtures, real
pipeline.

## Definition of done

- [ ] Suite green in notebook CI; matrix documented (which pairs covered)
- [ ] Failure isolation + restart semantics tested
- [ ] A deliberate regression (break sharing locally) fails the suite —
      demonstrated once in the PR description

## Files in scope

- crush-notebook tests + CI (crush-workspace repo)

## Gates

CRUSH-101.


## Dispatch

Imported from `workspace-meta/prompts/crush-backlog/CRUSH-102.txt` on 2026-08-24 so this tracked ticket contains the dispatch prompt metadata.

- Branch: `agent/horse/CRUSH-102`
- Repo: `crush-workspace`
- Turns: `60`
- Runner: `claude`
- Scope: this ticket file is the canonical implementation spec; read it before changing code.
- Discipline: verify the repro against current `main` first, commit and push each meaningful unit, and update this ticket with the result.
- Verification: satisfy this ticket's Definition of done, include test evidence, and quote the real post-commit `HEAD` hash.
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs` unless this ticket explicitly scopes them; flag `crush_cast::Function`/`Program` shape changes before landing.
- Halt: stop and DM foreman if gates are unmet, scope is wrong, the repro no longer exists, sandbox blocks required work, or budget is nearly exhausted.
