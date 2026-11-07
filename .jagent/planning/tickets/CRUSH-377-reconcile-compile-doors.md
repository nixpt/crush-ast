# CRUSH-377 — Reconcile the two compile doors on Python semantics

| Field | Value |
|-------|-------|
| **ID** | CRUSH-377 |
| **Priority** | P2 |
| **Status** | In Progress — part 1 merged in PR #407 (`a6bac3e6`, 2026-11-07); see Progress |
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

## Progress

**Part 1, PR #407 (altair):** the semantic checker on the CLI route now accepts `and` and `or` when both operands are known bools, and `**` on numeric or `Any` operands. `and` and `or` on anything else are still refused with `Logical operator or requires bool operands`, because they compile to a bool where Python yields an operand. Value semantics for `0 or 5` remain open. Tests: `crates/crush-lang-python/tests/cli_route_test.rs`.

**Fidelity matrix, `crush-run` route** (the audit's 116 programs, with #405 to #408 merged):

| | Audit | Now |
|---|---:|---:|
| Silently wrong | 12 | 6 |
| Refused loudly | 78 | 79 |
| Match CPython | 26 | 31 |

That matrix was run on PR branches merged locally, not on `main` itself. The six still silently wrong: tuples (2) and int dict keys (2), both on CRUSH-zorro-2, a redefined `def`, and `math` used without an import. `if []:` still reports `If condition must be bool`, and the walker door wasn't re-measured.
