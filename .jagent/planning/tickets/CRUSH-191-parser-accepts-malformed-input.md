# CRUSH-191 — Parser leniency: unterminated annotations swallow code; malformed syntax accepted; duplicates silently replace

| Field | Value |
|-------|-------|
| **ID** | CRUSH-191 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

Found while feeding malformed input to `crushc`:

- **Unterminated annotation eats code.** `print("a")` / `@wip {` / `print("b")` /
  `print("c")` prints `a`, `c`, exit 0 — `print("b")` vanished. `@decision "x" {`
  and `@module { purpose: }` are also accepted silently. One-line
  `@wip { intent: "t", todo: ["a"] }` fails with a confusing ``unexpected `,` ``.
  (`parse_wip_block` / `parse_decision_block`: unknown keys go through
  `skip_at_value`; a missing `}` is never reported.)
- **Accepted but malformed:** `fn f() -> { }`, `let x: = 1`, `try { } catch { }`,
  `"a" "b"`, `print(1) print(2)` on one line, `capability x`.
- **Duplicates:** a second `fn f`/`struct P` silently replaces the first
  (`functions.insert`, `parser/mod.rs:~389`); `fn f(a, a)` accepted; user-defined
  `fn len`/`fn print` silently ignored in favour of the builtin.
- **Unknown string escapes** drop the backslash: `"\q|\0|C:\dir"` → `q|0|C:dir`
  (`lexer.rs:~316`).
- **Writing an undeclared struct field** (`p.z = 1`) compiles and runs; the guide says it's a compile error.
- `fn main(x)` fails at run time with `stack underflow` instead of a compile error.
- `for x in 5` is not caught by the type checker.
- Deeply nested input (~2000 parens) overflows the parser stack and aborts (see CRUSH-76 fuzzing).

## Success criteria

- [ ] Each bullet is a compile error with a location (or, for escapes, either an
      error or a documented pass-through).
- [ ] A parser recursion limit with a clean error.
- [ ] Negative tests for each in `crush-frontend/tests/`.
