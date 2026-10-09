# CRUSH-184 — `crush-run`/`crush-walk-run` discard all prior output on a runtime error; walk-run exits 0

| Field | Value |
|-------|-------|
| **ID** | CRUSH-184 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

```crush
print("start")
let a = [1]
print(a[5])
```

`crush-run run x.crush` prints only `[runtime] array index out of range: 5 (len 1)`;
`start` is lost. Output is buffered and printed only on success. This makes every
runtime bug harder to debug. GitHub #91 item 3 fixed the same thing for crush-web
(`execute_with`/`Session`) but not for the native CLI.

`crush-walk-run` has the same problem, and also exits **0** on a runtime error
(`print("before"); print(1/0)` → only `Error: division by zero`, exit 0).

## Where

- `crates/crush-lang-sdk/src/bin/crush-run.rs:443` (`runtime.run(&program)?`), `:470` (`print_result`).
- `crates/crush-aot/src/bin/walk_run.rs:88-92`.

## Success criteria

- [ ] Output produced before an error is flushed (stream it, or print the partial
      buffer on error) in `crush-run`, `crush run` and `crush-walk-run`.
- [ ] `crush-walk-run` exits non-zero on runtime errors.
