# CRUSH-242 — `process.exec` returns a JSON string instead of a value

| Field | Value |
|-------|-------|
| **ID** | CRUSH-242 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-217 |
| **Estimated effort** | XS |
| **Filed by** | claude — 2026-10-09 scripting test drive (`tests/scripting_parity.rs`) |

## Problem

`process.exec(cmd, args)` returns the string `{"exit_code":0,"stderr":"","stdout":"…"}`, so
every call needs `json.parse(...)` before reading `exit_code`. And on the parsed map,
`r["exit_code"]` errors (CRUSH-217); only `r.exit_code` works. A missing command is an
uncatchable runtime error rather than a result with a failure code.

## Success criteria

- [ ] `process.exec` returns a map (`exit_code`, `stdout`, `stderr`).
- [ ] A command that can't be started is reported in the result (or a catchable error,
      once CRUSH-219 lands), not a run-ending error.
