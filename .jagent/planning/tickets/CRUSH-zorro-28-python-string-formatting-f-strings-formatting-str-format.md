# CRUSH-zorro-28 — Python: string formatting — f-strings, % formatting, str.format

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-28 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M6 (frontends) |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |
| **Filed by** | claude (vega) — 2026-10-15, Python `.py` probe (50 programs vs python3, main `83f6599`) |

## Problem

All three Python string-formatting forms fail (Probe programs: `/workspace/scratch/pyprobe/` (vega's box); run with `crush-walk-run <file>.py`.):

| program | crush |
|---|---|
| `f"{name} has {n} items, {n * 2.5:.2f}"` | `f-string interpolation not yet supported` |
| `"%s is %d years" % ("bo", 30)` | runtime `type error: expected numeric, got str` (`%` lowered as modulo) |
| `"{} + {} = {}".format(1, 2, 3)` | `method .format() with 3 argument(s) is not supported yet` |

f-strings are the most common of the three in modern code.

## Fix

- f-strings: lower `JoinedStr` / `FormattedValue` to concatenation of `str()` of each part; format
  specs (`:.2f`, `:>5`, `:,`, `!r`) via one `py.format(value, spec)` cap implemented once in crush-vm.
- `%`: when the left operand is a string, lower to `py.percent_format(fmt, args)` (at run time if the
  type isn't known statically) instead of numeric modulo.
- `str.format`: positional / indexed / named fields through the same spec code.

## Done when

- [ ] All three probe programs match python3 on scheduler + portable VM; AOT exact or loud.
- [ ] A cpython_corpus program covering the common specs (`.2f`, width/alignment, `!r`).
