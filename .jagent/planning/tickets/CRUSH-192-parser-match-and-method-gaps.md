# CRUSH-192 — Parser gaps: match-arm comma after block, negative/float patterns, method calls on non-variable receivers

| Field | Value |
|-------|-------|
| **ID** | CRUSH-192 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

- `let b = match 2 { 2 => { 7 }, _ => 0 }` → ``E-PP02 expected pattern, found `,` ``;
  works without the comma (`parser/mod.rs:~1803-1806` — block branch doesn't skip the comma).
- `-1 => …` and `1.5 => …` patterns rejected (`expected pattern`).
- Method calls only work on a bare variable receiver: `o.list.push(4)`,
  `a[0].push(4)`, `"abc".len()` → `Cannot call non-function` (`parser/mod.rs:~1399-1414`).
  `a.len()` / `a.contains(1)` compile but fail with `unknown capability: len` (related CRUSH-112).
- `let io = 5` makes `io.print(..)` resolve to a method call on the local.
- ``use util2`` says ``expected identifier after 'use', found `util2` `` — should list the accepted forms (`@mcp/@cap/@lang`, `mod.rs:~1952`).
- String slicing `s[1:3]` compiles then fails `expected array, got str` (`arr_slice` is
  array-only, `scheduler.rs:~1529`); `a[1:-1]` returns `[]` though `a[-1]` works.

Already tracked and still reproducing: lambdas (CRUSH-75/#78), map subscript
`o["a"]` (GAP-MAP-SUBSCRIPT — also hits `json.parse` output), multi-line struct
bodies (GAP-STRUCT-MULTILINE), `spawn`/`async` (CRUSH-34), imports (CRUSH-110).

## Success criteria

- [ ] Each bullet fixed with a test, or documented as unsupported with a clear error.
