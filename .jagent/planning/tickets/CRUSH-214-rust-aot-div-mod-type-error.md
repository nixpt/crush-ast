# CRUSH-214 — Rust AOT backend can't compile any program using `/` or `%` (E0308); keyword-named functions emit invalid Rust

| Field | Value |
|-------|-------|
| **ID** | CRUSH-214 |
| **Priority** | P0 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
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

- [ ] `/` and `%` compile and match interp output (incl. the CRUSH-213 edge cases).
- [ ] Function/variable names are mangled or raw-escaped; test with `loop`, `type`, `match`.
- [ ] A CI smoke test compiles every `examples/crush` program the backend claims to support.

## Update 2026-10-09 (AOT test drive, `main` `5755262`)

- Not only games: `let z = 0; io.print(5 / z)` fails to compile too. Any `/` or `%` whose
  operands aren't both literals goes through this closure.
- Across `examples/crush`, all 104 rustc errors (69 `%`, 35 `/`) come from these two
  lines; `--backend rustc` matches interp on 10 of 27 runnable examples, gcc on 14.
