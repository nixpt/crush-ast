# CRUSH-190 — Semantic/type and runtime errors in the CLIs carry no line/column

| Field | Value |
|-------|-------|
| **ID** | CRUSH-190 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-74 |
| **Estimated effort** | M |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

`[type] type error: Undefined variable: zz` has no location; the JSON form has
`"line":null,"col":null` (`crushc.rs` notes "Semantic errors don't carry source
coordinates"). Runtime errors in `crush-run`/`crush run` also have none
(`[runtime] division by zero`, JSON `file/line: null`), although CRUSH-74 is marked
Done — its DoD was met at library level but the CLIs never consult the source map.
The SPAWN error's `line N` is a CASM line, not a source line.
Polyglot walkers: undefined variables surface only at run time as
`load from uninitialised slot N` instead of a compile error naming the variable.

## Success criteria

- [ ] Semantic analyzer errors carry spans; `crushc` and `crush-run` render them with a caret.
- [ ] Runtime errors map the faulting IP back to a source line via the CRUSH-74 source map.
- [ ] `--message-format json` emits non-null `line`/`col` for both.
