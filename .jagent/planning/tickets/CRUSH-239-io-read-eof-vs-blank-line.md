# CRUSH-239 — `io.read()` can't tell a blank line from end of input

| Field | Value |
|-------|-------|
| **ID** | CRUSH-239 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-115 |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 scripting test drive (`tests/scripting_parity.rs`) |

## Problem

`io.read()` returns `""` both for an empty line and at end of input (the CRUSH-115
convention). The natural loop `while line != "" { … line = io.read() }` stops at the
first blank line, so a script summing a piped CSV with a blank line in the middle gives
a **silently wrong** answer. `tests/scripting_parity.rs` case `t03_csv_sum` pins it.

## Success criteria

- [ ] A way to detect end of input distinct from an empty line, e.g. `io.read()`
      returning `null` at EOF (breaking: decide and record it), or `io.read_line()` /
      `io.eof()` alongside.
- [ ] Same behaviour on every engine via `io_read.rs` (the CRUSH-114 rule).
- [ ] `t03_csv_sum` removed from `KNOWN_GAPS`.
