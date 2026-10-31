# CRUSH-197 — Rust walker: `println!` drops its arguments, tail expressions aren't returned, `..=` is exclusive

| Field | Value |
|-------|-------|
| **ID** | CRUSH-197 |
| **Priority** | P0 |
| **Status** | Done — PR #166 merged 2026-10-12 |
| **Phase** | M6 |
| **Assignee** | vega |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

- `fn main(){ println!("Hello"); }` prints an empty line: `println!`/`print!` emit
  `io.print` with `args: vec![]` (`crush-lang-rust/src/lower_stmt.rs:~146-152`;
  known in a test comment at `lib.rs:~130-132`, never ticketed). Every Rust program
  prints only blank lines.
- `fn f(n:i64)->i64{ n+1 }` returns `null` — the final expression is lowered as
  `ExprStmt`, not `Return`.
- `for i in 1..=4` sums to `6` (expected `10`).
- `match`, `loop`, `continue`, `vec!`, struct literals and `format!` are
  unsupported (they fail loudly — fine, but list them in the README).

## Success criteria

- [x] `println!`/`print!` with a format string and `{}` args produce the same output as rustc (`print!` still ends the line — no newline-free print capability).
- [x] Block tail expressions become the block's value / function return.
- [x] `..=` inclusive.
- [x] Output-comparison tests against `rustc` for a small corpus.

## Result (2026-10-12)

`agent/vega/CRUSH-197-rust-walker`. Fixed the three bugs and added `match` (literal / binding /
`_` / `|` patterns), `loop`, `continue`, `vec![..]`, `format!`, compound assignment `*= /= %=`
and a method map (`len`, `push`, `pop`, `to_string`, string and math methods; borrow/iterator
adapters pass through). Unsupported constructs — format width/precision, match guards, range
patterns, structs/impls, iterator chains — fail with a named error. `tests/rustc_corpus/` (9
programs) is compared with real `rustc` output, and a second test re-derives the `.out` files
with `rustc` so they can't drift. The supported subset is in the crate's rustdoc (the crate has
no README of its own).

## Survey 2026-10-31 (status)

PR #166 (`agent/vega/CRUSH-197-rust-walker`) merged 2026-10-12; all criteria ticked. Previous status: `In review`.
