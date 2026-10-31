# CRUSH-201 — C walker: `for` with `continue` loops forever; `printf` formats and escapes not applied

| Field | Value |
|-------|-------|
| **ID** | CRUSH-201 |
| **Priority** | P0 |
| **Status** | Done — PR #170 merged 2026-10-12 |
| **Phase** | M6 |
| **Assignee** | vega |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

- `for(int i=0;i<3;i++){ if(i==1) continue; printf("%d",i); }` → `instruction quota exceeded`.
  The update step is appended to the end of the while-body, so `continue` skips it
  (`crush-lang-c/src/lib.rs:~267-300`).
- `printf("%d\n", 7)` → `%d\n7` with a literal backslash-n; string literal escapes
  aren't decoded.
- Arrays, structs and pointers are broken.

## Success criteria

- [x] `for` lowers so `continue` runs the update (use a continue-target block).
- [x] `printf` handles `%d %s %c %%` and `\n \t \\` escapes. `%f` (and widths/precision/hex) fail loudly — no float-formatting capability yet.
- [x] Output tests against `gcc`.

## Result (2026-10-12)

`agent/vega/CRUSH-201-c-walker-loud`. On `main` 0/13 test programs matched gcc; now 13/13 of the
supported subset do, and the rest fail by name. Fixed: `for`+`continue` hang, `do-while` (copied
body; `continue` didn't compile), printf formats + escapes, **every `else` branch was dropped**
(`else_clause` hit the catch-all), arrays (`{..}` initialisers, `a[i] = v`, `int a[N]`), struct
values, `char` literals, comparisons/`!` yield ints 1/0. Unknown statements and expressions are now
errors instead of being dropped / becoming null. New `io.write` capability (no newline) in both
VMs and both AOT backends — usable by Rust `print!` and bash `echo -n` next. Pointers, `sizeof`,
`goto`, `%f` remain unsupported (loud).

## Survey 2026-10-31 (status)

PR #170 (`agent/vega/CRUSH-201-c-walker-loud`) merged 2026-10-12; all criteria ticked. Previous status: `In review`.
