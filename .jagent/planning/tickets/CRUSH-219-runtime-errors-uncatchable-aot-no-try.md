# CRUSH-219 — VM runtime errors can't be caught by `try`; AOT rejects `try`/`throw`/structs and has no call-depth limit

| Field | Value |
|-------|-------|
| **ID** | CRUSH-219 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

- `try { print(1 / z) } catch e { … }` with `z = 0` is not caught in interp or FastVM
  (JIT crashes, CRUSH-213). Same for Python `try: 1/0 except ZeroDivisionError:` via walkers.
- All AOT backends reject `enter_try` (`cannot compile enter_try`) and `new_struct`, so
  17 of 44 examples are unsupported on AOT.
- AOT has no call-depth limit: 100,000-level recursion aborts with a native stack
  overflow; clang overflows on `tail(10000)` where gcc succeeds.

## Success criteria

- [ ] Decide (and document) whether VM errors are catchable; if yes, raise them through the
      same unwinding path as `throw` in every engine.
- [ ] AOT support for try/throw and structs, or a clear "unsupported on AOT" list in the docs.
- [ ] AOT call-depth counter mirroring `--max-call-depth`.
