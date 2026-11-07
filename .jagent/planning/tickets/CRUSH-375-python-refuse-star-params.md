# CRUSH-375 — Python walker: refuse `*args`, `**kw` and keyword-only params it can't honour

| Field | Value |
|-------|-------|
| **ID** | CRUSH-375 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | Maya audit |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-zorro-32 |
| **Estimated effort** | S |
| **Filed by** | altair — 2026-11-06, from satya's Maya-through-Crush audit |

## Problem

These params vanish from the signature (77 uses in the audited corpus). A later read fails with an unrelated error, and an extra argument is accepted on the walker door. CRUSH-zorro-32 tracks implementing them; this ticket is the loud refusal until then.

Measured at `main` `495d9995` (v0.11.1) by the 2026-11-06 Maya-through-Crush audit.

## Evidence

On `crush-run`, `def f(a, *rest)` called with 3 arguments reports `Function 'f' expects 1 arguments, found 3`, and reading `rest` reports `Undefined variable: rest`. `def f(**kw)` with no use compiles and runs. Re-run independently on `495d9995` through `python_walker` then `crush-run run`. The walker-door behaviour is from the audit.

## Success criteria

- [ ] The walker refuses these params at walk time and names the construct, until CRUSH-zorro-32 lands
