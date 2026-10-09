# CRUSH-248 — WASM walker: exported functions renamed, but calls to them still say `func_N`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-248 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M11 |
| **Assignee** | unassigned |
| **Dependencies** | none (fold into CRUSH-246 if done together) |
| **Estimated effort** | XS |
| **Filed by** | claude — 2026-10-09 CASM↔WASM test drive |

## Problem

`walk_wasm` names a function after its export (`add`) but emits calls to it as
`func_<index>`, so any exported function that is also called internally is unresolvable:
`(call $add …)` → `Error: unknown capability: func_1`.

## Success criteria

- [ ] One index → name map used for both definitions and calls.
- [ ] Test: a module whose `_start` calls an exported function.
