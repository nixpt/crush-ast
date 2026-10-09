# CRUSH-189 — Optimizer: `2*e`→`e+e` duplicates side effects; identity rewrites ignore types; fold overflow panics; `catch` sees stale constants

| Field | Value |
|-------|-------|
| **ID** | CRUSH-189 |
| **Priority** | P0 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
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

- [ ] Strength reduction only when `e` is a pure, side-effect-free expression (or drop it).
- [ ] Identity rewrites only when both operand types are known numeric (and the
      dropped operand is pure); `0*float` keeps float.
- [ ] Folding uses `checked_*` and leaves the expression unfolded on overflow.
- [ ] Variables assigned anywhere in a `try` body are invalidated for the handler.
- [ ] Differential test: every `examples/crush/*.crush` gives identical output with and without `-O`.
- [ ] Close #92's game_of_life half.
