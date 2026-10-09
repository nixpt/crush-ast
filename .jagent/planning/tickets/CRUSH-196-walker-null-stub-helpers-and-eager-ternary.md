# CRUSH-196 — Walkers: `__crush_ifexpr__`/`__crush_slice__`/`__crush_contains__`/`__crush_is__` are null stubs; ternary lowered as an eager call

| Field | Value |
|-------|-------|
| **ID** | CRUSH-196 |
| **Priority** | P0 |
| **Status** | Backlog |
| **Phase** | M6 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

1. `crush-walk-run` registers stub host caps for `__crush_ifexpr__`,
   `__crush_slice__`, `__crush_contains__`, `__crush_is__` (and others) that just
   return `Null`. No real implementation exists anywhere; the same stubs are in
   `crush-lang-python/src/sdk.rs:~31`, so the tests can't notice.
   - Python: `print(5 if True else 6)` → `null`; `"hello"[1:4]` → `null`;
     `"a" in {"a":1}` → `null`; `None is None` → `null`.
   - Rust: `let s = if x > 10 {"big"} else {"small"}` → `null`.
2. Even with a real implementation, the ternary/if-expression is lowered as
   `Call __crush_ifexpr__(cond, a, b)` — **both branches are always evaluated**.
   JS `function fib(n){return n<2?n:fib(n-1)+fib(n-2)} console.log(fib(15))` →
   `call depth quota exceeded (256)`; expected `610`. (The C walker lowers `?:`
   correctly — copy that.)

## Where

- `crates/crush-aot/src/bin/walk_run.rs:66-79` (stubs)
- `crush-lang-js/src/lower_swc.rs:~773`, `crush-lang-python/src/lower_expr.rs:~118`,
  `crush-lang-rust/src/lower_expr.rs:~141` (eager `__crush_ifexpr__`)

## Success criteria

- [ ] Ternary/if-expressions lower to a CAST conditional (branching), not a call.
- [ ] Slice / `in` / `is` lower to real ops or real shared caps (one implementation,
      every backend — CRUSH-114 rule); no stub returns `Null` silently.
- [ ] Walker tests assert on *program output*, not just CAST shape.
