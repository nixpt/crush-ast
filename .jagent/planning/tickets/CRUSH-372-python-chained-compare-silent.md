# CRUSH-372 — Python walker: chained comparisons keep only the first pair

| Field | Value |
|-------|-------|
| **ID** | CRUSH-372 |
| **Priority** | P1 |
| **Status** | Done — merged in PR #405 (`83989ddb`, 2026-11-07); see Resolution |
| **Phase** | Maya audit |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |
| **Filed by** | altair — 2026-11-06, from satya's Maya-through-Crush audit |

## Problem

`0 <= x < 3` evaluates only `0 <= x`, with no error and on both compile doors. In the audited corpus that is 59 sites in 18 files, including core simulation code. A wrong bounds check here is silent, so this is the highest-priority item from the audit.

Measured at `main` `495d9995` (v0.11.1) by the 2026-11-06 Maya-through-Crush audit.

## Evidence

```python
x = 5
print(0 <= x < 3)        # CPython: False, Crush: True
if 0 <= x < 3: print(1)   # CPython: skips, Crush: prints 1
```
Re-run independently on `495d9995` through `python_walker` then `crush-run run`.

## Success criteria

- [x] `a op b op c` lowers to `a op b and b op c` with `b` evaluated once, or is refused loudly
- [x] Both doors agree with CPython, including when `b` has side effects

## Resolution

PR #405 (altair) lowers a chain to `and` of all its links, in order, with short-circuiting. The middle operand is lowered twice, so it must be a name, constant, attribute, subscript, or arithmetic on those. Anything else, such as a call, is refused with `chained comparison with a middle operand that may have side effects is not supported yet`.

- **Walker door:** corpus program `25_chained_compare`, checked against CPython.
- **CLI route:** PR #407 made the checker accept `and` on bools, after which chains work there too. `cli_route_test.rs` covers it with `chained_comparisons_match_cpython` and `chained_comparison_with_a_call_in_the_middle_is_refused`. Both fail with the lowering from before #405.
- **Refused, not implemented:** a call in the middle of a chain, for example `500 < len(land) < 3500`, is refused, as the success criteria allow. In a copy of the Maya source, 16 of 119 chains are refused this way. Hoisting the operand into a temp would make them work.
