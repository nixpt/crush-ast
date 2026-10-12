# CRUSH-200 — Python walker semantics: `/`, `//`, `%`, `range` step, dicts, `print` separators, silently-ignored constructs

| Field | Value |
|-------|-------|
| **ID** | CRUSH-200 |
| **Priority** | P0 |
| **Status** | In Progress (semantics slice landed on `agent/nixp/CRUSH-200`) |
| **Phase** | M6 |
| **Assignee** | nixp |
| **Dependencies** | none |
| **Estimated effort** | M |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

Compared against `python3`:

- `7/2` → `3` (expected `3.5`); `-7//2` → `-3` (expected `-4`); `-7%2` → `-1` (expected `1`).
- `for j in range(1,10,3)` → `1..8` (expected `1 4 7`): VM `make_range` handles only 2 args
  and turns non-int args into `0`/`100` (`crush-vm/src/portable_vm.rs:~1673-1688`).
- `d["c"]=3` → `type error: expected array, got map`; `for k in d` → `expected array or string, got map`.
- `print(1, 2, "a")` → `12a` (no separator; same for JS `console.log(a,b)`, Go `fmt.Println(a,b)`);
  `True`/`None` print as `true`/`null`; a caught exception prints as a raw map.
- Silently dropped: `for/while … else:`; decorators (`@twice def g(): return 21` → `21`, expected `42`).
- Default args: `def g(n, greeting="Hello")`; `g("Bob")` → `stack underflow` at run time.
- VM errors can't be caught: `try: 1/0 except ZeroDivisionError:` → uncaught.
- `import math; math.sqrt(16)` → `Unsupported CVM1 opcode: math_sqrt` (related CRUSH-112).
- CRUSH-35 "closed" claims partly wrong: tuple unpacking `a, b = 1, 2` → `unsupported assignment target`.
- `**`, `&`, `<<` → `CAST→CASM: Unsupported op` (loud — fine, but tracked here).

## Success criteria

- [x] Python numeric operators follow Python semantics (float `/`, floor `//`, sign-of-divisor `%`).
      Shared `math.truediv` / `math.floordiv` / `math.floormod` caps + `arithmetic::{true,floor}_*_values`.
- [x] `range(start, stop, step)` incl. negative step.
      Shared `make_range::make_range_values` (1–3 int args; zero step errors; no silent 0/100 coercion).
- [x] Dict set/iterate; `print` sep/repr for bool/None.
      `ARR_GET`/`arr_set` accept maps; `for` already uses `map_keys`. `py.print` joins with spaces and spells `True`/`False`/`None`.
- [x] Unsupported constructs (else-clauses, decorators) fail loudly until implemented.
      Also default-arg defs (was silent → stack underflow).
- [x] Output tests against `python3` (`crush-lang-python` `crush200_*` tests).

## Still open (same ticket / follow-ups)

- [ ] Default args actually implemented (currently loud-fail only).
- [ ] VM arithmetic errors catchable as `ZeroDivisionError`.
- [ ] `import math; math.sqrt` (CRUSH-112).
- [ ] Tuple unpacking `a, b = 1, 2`.
- [ ] `**` / bitwise ops (still loud `Unsupported op` — fine).
