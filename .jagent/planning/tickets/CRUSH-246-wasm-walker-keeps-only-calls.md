# CRUSH-246 — WASM walker is a stub: keeps only `call` instructions, drops all computation

| Field | Value |
|-------|-------|
| **ID** | CRUSH-246 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M11 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-247 (binary input) to test end to end |
| **Estimated effort** | L |
| **Filed by** | claude — 2026-10-09 CASM↔WASM test drive |

## Problem

`crush_lang_wasm::walk_wasm` reads each function body and keeps only `Operator::Call`
(as an argument-less `Call`) and stops at the first `End`. Everything else — constants,
arithmetic, locals, parameters, `block`/`loop`/`br`/`if`, loads/stores, return values — is
dropped. A WASI `fd_write` becomes `io.print()` with no argument. Its two unit tests pass
because they check a module that is nothing but a call.

Live (`crush-walk-run`, modules assembled from WAT; Node's WebAssembly/WASI as reference):

| Module | Node | Crush |
|---|---|---|
| `add(40,2)` | `42` | `unknown capability: func_1` |
| factorial loop | `3628800` | `load from uninitialised slot 0` |
| recursive fib | `6765` | `call depth quota exceeded (256)` (the `if` base case was dropped) |
| WASI hello / clang-compiled C | output | never walked (CRUSH-247) |

The crate describes itself as "parses Wasm modules into CAST IR"; CRUSH-238 covers the other
walkers overclaiming in the same way.

## Success criteria

- [ ] Stack-to-tree translation for the MVP numeric subset: `i32`/`i64` consts and
      arithmetic/compare, `local.get/set/tee`, params and results, `block`/`loop`/`br`/
      `br_if`/`if`/`return`, direct calls with arguments.
- [ ] Linear memory + `data` segments, enough for WASI `fd_write` to print the iovec bytes.
- [ ] Unsupported operators fail the walk with the operator name (not dropped).
- [ ] A parity test: a set of `.wat` modules run through `crush-walk-run` must print what
      Node's WebAssembly prints (or be listed as known gaps).
