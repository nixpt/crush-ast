# CRUSH-377 — Reconcile the two compile doors on Python semantics

| Field | Value |
|-------|-------|
| **ID** | CRUSH-377 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | Maya audit |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-384 |
| **Estimated effort** | L |
| **Filed by** | altair — 2026-11-06, from satya's Maya-through-Crush audit |

## Problem

`Compiler::compile` (used by the parity tests) and the CLI route (`python_walker`, `.cast.json`, `crush-run`) disagree. Green parity tests therefore say little about the CLI route. In the audit's 116-program fidelity matrix, 17 are silently wrong on the walker door and 12 on `crush-run`.

Measured at `main` `495d9995` (v0.11.1) by the 2026-11-06 Maya-through-Crush audit.

## Evidence

On `crush-run`: `0 or 5` reports `Unknown operator: or`, `2 ** 3` reports `Unknown operator: **`, and `if []:` reports `If condition must be bool`. Re-run independently on `495d9995` through `python_walker` then `crush-run run`. The walker-door results, such as `x or y` returning a bool, are from the audit.

## Success criteria

- [ ] Both doors give the same result, or the same refusal, on the fidelity matrix
- [ ] Parity tests exercise the CLI route
