# CRUSH-241 — Scripts can't write to stderr; `crush-run` always prints `[steps=…]`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-241 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | XS |
| **Filed by** | claude — 2026-10-09 scripting test drive (`tests/scripting_parity.rs`) |

## Problem

- There is no way to print to stderr (`io.eprint`), so a script's errors and diagnostics
  mix with its data on stdout, and `script | next` can't separate them.
- Every `crush-run run` prints `[steps=N, stack=M]` to stderr, which is noise for scripts
  (it's useful when debugging).
- Capability failures read `unknown capability: fs.cat: fs.cat x: No such file…` (see the
  TASKS.md issue): the prefix is wrong for a registered capability that failed.

## Success criteria

- [ ] `io.eprint(x)` (ambient, `stderr/write`), streamed like `io.print`.
- [ ] `[steps=…]` only with a flag (e.g. `--stats`) or `CRUSH_DEBUG`, not by default.
- [ ] A failing registered capability is reported as `<cap>: <message>`.
