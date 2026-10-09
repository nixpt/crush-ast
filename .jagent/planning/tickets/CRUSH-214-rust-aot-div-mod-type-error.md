# CRUSH-214 — Rust AOT backend can't compile any program using `/` or `%` (E0308); keyword-named functions emit invalid Rust

| Field | Value |
|-------|-------|
| **ID** | CRUSH-214 |
| **Priority** | P0 |
| **Status** | Done (2026-10-09, PR #126) |
| **Phase** | M1 |
| **Assignee** | claude |
| **Dependencies** | none |
| **Estimated effort** | XS |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

`crates/crush-aot/src/codegen.rs:~502-503`: the division closure returns `div_zero()`
(an `i64`) where a `RuntimeValue` is expected → rustc E0308. `crush-aotc run --backend rustc`
fails on 8/8 game examples.

A Crush function named `loop` (any Rust keyword) emits `fn loop(...)` — names need
`r#` escaping or mangling.

## Success criteria

- [x] `/` and `%` compile and match interp output (incl. the CRUSH-213 edge cases).
- [x] Function/variable names are mangled or raw-escaped; test with `loop`, `type`, `match`.
- [ ] A CI smoke test compiles every `examples/crush` program the backend claims to support.

## Update 2026-10-09 (AOT test drive, `main` `5755262`)

- Not only games: `let z = 0; io.print(5 / z)` fails to compile too. Any `/` or `%` whose
  operands aren't both literals goes through this closure.
- Across `examples/crush`, all 104 rustc errors (69 `%`, 35 `/`) come from these two
  lines; `--backend rustc` matches interp on 10 of 27 runnable examples, gcc on 14.

## Resolution

- `div_zero()`/`div_zero_f()` now return `!`; int `/` and `%` use `checked_div`/`checked_rem`,
  so `i64::MIN / -1` is the backend's `arithmetic overflow` error, not a panic in the `.so`.
- Every generated function is `crush_fn_<name>` (entry `crush_fn_main`), so Crush names
  can't be Rust keywords or collide with runtime helpers. (`match` is a Crush keyword, so
  the test uses `loop`, `type`, `impl`, `ref`, `bin_add`, `negate`.)
- Tests: `crush-aot/tests/integration.rs` `test_aot_div_and_mod_of_runtime_values_compile`,
  `test_aot_function_names_that_are_rust_keywords_or_helpers` (both fail before the fix).
- Live: `--backend rustc` now matches `crush-run` on 18 of 27 runnable examples (was 10),
  including all 8 games. Still open: the third criterion (a CI smoke test over
  `examples/crush`) — tracked with CRUSH-220.
