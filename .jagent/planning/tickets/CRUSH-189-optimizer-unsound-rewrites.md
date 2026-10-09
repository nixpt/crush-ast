# CRUSH-189 — Optimizer: `2*e`→`e+e` duplicates side effects; identity rewrites ignore types; fold overflow panics; `catch` sees stale constants

| Field | Value |
|-------|-------|
| **ID** | CRUSH-189 |
| **Priority** | P0 |
| **Status** | Done (2026-10-09, branch `ccr-12236bee-uj4oar`) |
| **Phase** | M1 |
| **Assignee** | claude |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build); GitHub #92 (game_of_life) |

## Problem

The optimizer runs on `crushc -O` and **always** on `crush-run x.crush` / `crush run`
(CRUSH-141), so each of these silently changes program behaviour:

1. **Strength reduction duplicates `e`.** `2*e` / `e*2` → `e+e` evaluates `e` twice.
   ```crush
   fn f() { print("called") return 5 }
   print(2 * f())        // prints "called" twice
   ```
   This is the root cause of #92's `game_of_life` hang: `return 2 * pow2(n - 1)`
   becomes exponential (286,738 steps unoptimized, >50M optimized).
2. **Identity rewrites ignore types and side effects.**

   | Expr | no `-O` | `-O` |
   |---|---|---|
   | `s + 0` (s = `"a"`) | `a0` | `a` |
   | `0 + "s"` | `0s` | `s` |
   | `0 * f()` | calls `f` | call dropped |
   | `0 * 2.5` | `0.0` | `0` |
   | `"ab" * 2` (param) | type error | `abab` |
3. **Constant folding panics on overflow.**
   `let a = 9223372036854775807` / `print(a + 1)` → `panicked at optimizer.rs:378:37:
   attempt to add with overflow` (exit 101); unoptimized gives
   `[runtime] arithmetic overflow`. Release builds would wrap silently.
   `i64::MIN / -1` same pattern.
4. **`catch` handler sees constants from before the `try`.**
   ```crush
   let t = 1
   try { t = 2  throw "x" } catch e { print(t) }   // prints 1, expected 2
   ```
   `handler_consts = consts.clone()` doesn't kill variables assigned in the try body
   (`optimizer.rs:~226`). Same class as CRUSH-143 (if-branches, Done).

## Where

`crates/crush-frontend/src/optimizer.rs:~226`, `~316-370`, `~378-404`.

## Success criteria

- [x] Strength reduction only when `e` is a pure, side-effect-free expression (or drop it).
- [x] Identity rewrites only when both operand types are known numeric (and the
      dropped operand is pure); `0*float` keeps float.
- [x] Folding uses `checked_*` and leaves the expression unfolded on overflow.
- [x] Variables assigned anywhere in a `try` body are invalidated for the handler.
- [x] Differential check: every `examples/crush/*.crush` gives identical output with and without `-O`
      (34/34 compilable examples, run by hand — not yet a CI test).
- [x] Close #92's game_of_life half.

## Resolution

- Removed the algebraic-identity block (`x*0`, `0*x`, `x*1`, `1*x`, `x+0`, `0+x`,
  `2*x`→`x+x`) instead of making it type-aware: the optimizer has no type or purity
  information, and literal operands are already covered by constant folding. `n * 2`
  as `n + n` was never cheaper on this VM (an extra `load`).
- Int folding uses `checked_add/sub/mul/div/rem`; unary `-` uses `checked_neg`. On
  overflow the expression is left for the runtime to report.
- `TryCatch`: variables assigned anywhere in the body are removed from the handler's
  constants.
- Regressions: `crates/crush-lang-sdk/tests/implicit_return_and_optimizer_test.rs`
  (all fail before the fix); `optimizer_tests.rs::mul_by_two_is_not_rewritten`.
- Live: awesome-crush `games/game_of_life.crush` completes in 286,738 steps (same as
  unoptimized), under the default 1M quota.
