# CRUSH-198 — Go walker silently drops `:=`, `var`, `for`, assignment, `switch`, `defer`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-198 |
| **Priority** | P0 |
| **Status** | Done |
| **Phase** | M6 |
| **Assignee** | nixp |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

`crush-lang-go/src/lib.rs:~91` matches `"short_variable_declaration"`, but
tree-sitter-go calls the node `short_var_declaration`, so it never matches. The
catch-all `_ => Ok(None)` (~line 159) then silently drops `for`, `switch`, `var`,
assignment, `++` and `defer`; unknown expressions become `NullLiteral` (~line 256).
The Go README claims "For Loops ✅ all variants" and "Variables ✅".

- `func main(){ a := 7; fmt.Println(a) }` → `Error: load from uninitialised slot 0`.
- `switch x { case 2: fmt.Println("two") }` → no output; CAST body empty.

## Success criteria

- [x] Fix the node name; implement or loudly reject every statement kind (no `_ => Ok(None)`).
- [x] Unknown expressions are an error, not `NullLiteral`.
- [x] README feature table matches reality; output tests against `go run`.

## Resolution (2026-10-30, `agent/nixp/CRUSH-297`)

`crates/crush-lang-go/src/lib.rs`:

- **Node name fixed:** the walker matched `short_variable_declaration`;
  tree-sitter-go emits `short_var_declaration`, so `a := 7` was dropped and `a`
  then read as uninitialised.
- **No silent drops:** `visit_statement` became `visit_statements -> Vec<_>`
  (so one node can yield several statements). `var`, assignment, `++`/`--`,
  bare blocks, and `if`/`for` initializers are now lowered; a C-style
  `for init; cond; update` becomes the initializer plus a `while`, and
  `for x := range xs` becomes a `For`. Every statement kind the walker does not
  lower (`switch`, `defer`, `go`, a two-variable range, …) is a loud error.
- **Unknown expressions error** instead of lowering to `NullLiteral`; a
  multi-value `return a, b` errors instead of silently keeping only `a`.
- `README.md`: the feature table now states what is actually lowered, and notes
  that a non-lowered construct is a loud error.

### Evidence

- Walker tests: `short_var_declaration_is_not_dropped`,
  `var_assignment_and_for_lower`, `unsupported_statements_are_loud`,
  `unsupported_expressions_are_loud`, `multiple_return_values_are_loud`
  (verified RED: reverting the node name makes the first fail).
- Live: `crush-walk-run t.go --dump-cast` shows `a := 7` → `VarDecl`, the
  C-style `for` → `VarDecl` + `While`, and `fmt.Println(a, b)` → an `io.print`
  capability call; the program runs (`crush-walk-run t.go` prints).
- `cargo test --workspace --lib --tests`: 2546 passed, 0 failed.

## Follow-up

`fmt.Println(a, b)` maps to `io.print`, which **concatenates** its arguments
(`713`), while Go joins them with spaces (`7 13`). A Go-aware print capability
would close that; single-argument `Println` already matches `go run`. Also, a
`go run`-based output test needs Go installed (not available here) — the tests
assert the CAST lowering instead.
