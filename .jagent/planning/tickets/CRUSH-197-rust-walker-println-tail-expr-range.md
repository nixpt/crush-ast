# CRUSH-197 — Rust walker: `println!` drops its arguments, tail expressions aren't returned, `..=` is exclusive

| Field | Value |
|-------|-------|
| **ID** | CRUSH-197 |
| **Priority** | P0 |
| **Status** | Backlog |
| **Phase** | M6 |
| **Assignee** | unassigned |
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

- [ ] `println!`/`print!` with a format string and `{}` args produce the same output as rustc.
- [ ] Block tail expressions become the block's value / function return.
- [ ] `..=` inclusive.
- [ ] Output-comparison tests against `rustc` for a small corpus.
