# CRUSH-201 — C walker: `for` with `continue` loops forever; `printf` formats and escapes not applied

| Field | Value |
|-------|-------|
| **ID** | CRUSH-201 |
| **Priority** | P0 |
| **Status** | Backlog |
| **Phase** | M6 |
| **Assignee** | unassigned |
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

- [ ] `for` lowers so `continue` runs the update (use a continue-target block).
- [ ] `printf` handles `%d %s %f %c %%` and `\n \t \\` escapes.
- [ ] Output tests against `gcc`.
