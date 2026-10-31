# CRUSH-184 — `crush-run`/`crush-walk-run` discard all prior output on a runtime error; walk-run exits 0

| Field | Value |
|-------|-------|
| **ID** | CRUSH-184 |
| **Priority** | P1 |
| **Status** | Done |
| **Phase** | M1 |
| **Assignee** | nixp |
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

- [x] Output produced before an error is flushed (stream it, or print the partial
      buffer on error) in `crush-run`, `crush run` and `crush-walk-run`.
- [x] `crush-walk-run` exits non-zero on runtime errors.

## Resolution (2026-10-30, `agent/nixp/CRUSH-299`)

- **`crush-run`** was already fixed: it streams output through
  `Runtime::run_streaming` (`crates/crush-lang-sdk/src/bin/crush-run.rs`), so
  output printed before an error is on stdout, and it exits non-zero via `?`.
  `crush run FILE` delegates to `crush-run run FILE` and inherits that.
- **`crush-walk-run`** (`crates/crush-aot/src/bin/walk_run.rs`) buffered run 0's
  output and replaced it with `Error: …` on failure. It now streams run 0's
  output through `run_with_caps_streaming` (so nothing printed before an error
  is lost) and returns an error — exiting non-zero — when the run fails.

### Evidence

- `crates/crush-aot/tests/walk_run_cli.rs` spawns the binary: a Python loop that
  prints `1\n2\n3` then reads past the end keeps that output on stdout **and**
  exits non-zero; a successful run exits zero. Verified RED (the output was
  replaced by `Error: …`).
- Live: `crush-walk-run loop.py` prints `1\n2\n3` then `Error: array index out
  of range: 3 (len 3)`, exit 1 (was: only the error, exit 0).
- `cargo test --workspace --lib --tests`: 2551 passed, 0 failed.
