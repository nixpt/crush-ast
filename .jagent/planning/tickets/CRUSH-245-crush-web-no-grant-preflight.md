# CRUSH-245 — `crush-web` runs programs that need ungranted capabilities, then fails partway with "unknown capability"

| Field | Value |
|-------|-------|
| **ID** | CRUSH-245 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M8 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-243 (preflight), CRUSH-241 (error label) |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 CASM↔WASM test drive |

## Problem

Natively, `crush-run run` refuses a program before it starts and lists every missing grant
(CRUSH-243, `Runtime::missing_grants`). In the browser the same program starts, prints its
first lines and dies at the first ungranted call:

```
fs_test.crush   web:    "Testing fs capabilities\nTest 1: fs.pwd\n" + runtime error: unknown capability: fs.pwd
                native: [capabilities] … needs capabilities this run does not grant; nothing was run:
                          --fs: fs.cd, fs.mkdir, fs.pwd, fs.rm, fs.touch
```

Seven corpus programs behave this way (`async_test`, `fs_test`, `lang_test`, `phase2_3_test`,
`polyglot_braces`, `repl_test`, `text_tools_test`). "unknown capability" is also wrong for a
known capability the host doesn't provide.

## Success criteria

- [ ] `execute`/`execute_with`/`Session`/`check` report the missing capabilities before
      running (`{ ok: false, missing: [...] }` / `status: "error"`), using the same
      `effects::missing_grants` the native preflight uses.
- [ ] The message says the browser can't provide them (no grant flag to suggest).
- [ ] `check()` returns the missing list too, so an editor can flag it as you type.
