# CRUSH-260 — JS walker: `.length` on strings/arrays; JS number semantics for unboxed ints

| Field | Value |
|-------|-------|
| **ID** | CRUSH-260 |
| **Priority** | P1 |
| **Status** | In Progress — the `.length` half landed in PR #164 (2026-10-12); the number-policy half is open |
| **Phase** | M6 |
| **Assignee** | unassigned |
| **Dependencies** | relates to CRUSH-199 (JS walker gaps: switch/finally/`/`) |
| **Estimated effort** | S-M |
| **Filed by** | claude (vega) — 2026-10-12 JS-vs-Node benchmark (CRUSH-261) |

## Problem

1. **`.length`**: `s.length` (string) and `a.length` (array) lower to a field lookup and yield `null`, so
   `total = total + out.length` fails on every engine (`arithmetic on non-numeric operands (Int(0), Null)`).
   The benchmark's `strings.js` can't run.
2. **Numbers**: JS numbers are IEEE doubles; integer literals lower to Crush `Int` (i64 with overflow errors) —
   correct for the benchmark programs but not in general (`2**53 + 1`, `1/2`: see CRUSH-199). CRUSH-257's
   unboxed `int64_t` locals are only valid where JS semantics agree.

## Success criteria

- [ ] `x.length` lowers to the length op for strings and arrays (and stays a field read for objects with a
      `length` key — decided by type at run time, or a `len_or_field` op).
- [ ] Number policy documented and implemented: integer arithmetic while results stay exact (|x| < 2^53), float
      otherwise; `/` is float division (CRUSH-199). Tests against Node for the edge cases.
- [ ] `strings.js` from CRUSH-261 matches Node on every backend.

## Survey 2026-10-31 (status)

PR #164 says it "implements the `.length` half"; the number half is listed under "Not in this PR". Previous status: `Backlog`.
