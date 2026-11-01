# CRUSH-221 — Float literals with exponents (`1.5e10`, `1e-7`) don't parse

| Field | Value |
|-------|-------|
| **ID** | CRUSH-221 |
| **Priority** | P2 |
| **Status** | In review |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | XS |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

`print(1.5e10)` and `print(1e-7)` are parse errors in every engine. Other languages'
walkers can produce these values, and AOT C/`%g` prints them in this form.

## Success criteria

- [x] Lexer accepts `[0-9]+(\.[0-9]+)?[eE][+-]?[0-9]+`; tests; tree-sitter grammar updated (CRUSH-207).

## Resolution

**Root cause.** `read_number` (`crates/crush-frontend/src/parser/lexer.rs`) had no
exponent branch, so `1e10` lexed as `Int(1)` followed by `Ident(e10)`, and the
parser failed on `e10`. Every engine shares this lexer, so all of them failed.

**Fix.**
- `crates/crush-frontend/src/parser/lexer.rs`, `read_number`: an `e`/`E` that
  `exponent_at` confirms (optional sign, then a digit) starts a decimal exponent
  on both int and fraction mantissas. At most one exponent per literal.
- Same file, the `.` arm: `1.e5` (dot, then an exponent shape) is an
  `InvalidNumber` error.
- Same file, a new arm: an `e`/`E` glued to digits that does not complete an
  exponent is an `InvalidNumber` error naming the whole literal.
- `crates/crush-frontend/src/render.rs` (`--emit ast`): float literals render with
  Debug formatting. Display wrote `1e20` as 21 bare digits, which re-lexes as an
  out-of-range Int, so the rendered source did not re-compile.
- `crates/tree-sitter-crush/grammar.js`: `float_literal` accepts the exponent
  forms. `src/parser.c` and `src/grammar.json` regenerated with tree-sitter CLI
  0.25.10 (ABI 15, built from crates.io because the npm package's binary download
  was unavailable). `src/tree_sitter/parser.h` kept at HEAD.

**Edge-case decisions.**

| Input | Result |
|---|---|
| `1e10`, `1E10`, `1.5e10`, `1.5E10`, `1e-7`, `1E-7`, `2.5e+3`, `2.5E+3`, `0e0` | one Float |
| `1e`, `1e+`, `1E-`, `1.5e`, `1.5e+`, `1ex`, `3e`, `1e5e5` | `invalid number literal` error naming the literal |
| `1.e5`, `1.e+5` | `invalid number literal` error (a fraction needs digits after the `.`, as `1.` never was a float) |
| `1.e` (no digit shape), `1.foo` | unchanged: `Int Dot Ident` (field access) |
| `.5` | unchanged: `Dot Int` (parse error, as before) |
| `1e400` | `inf` (Rust `parse` overflow is not an error, matching Python) |

**Print format (not changed).** Non-integral floats print with Rust `Display` on
every runtime, so `1e-7` prints `0.0000001`, not CPython's `1e-07`. This was true
for decimal literals before this ticket and is a separate, all-floats decision.
The test pins the engine text and checks that it parses back to the literal value.

**Tests.**
- `crates/crush-frontend/src/parser/lexer.rs`:
  `test_lex_float_with_exponent`, `test_lex_malformed_exponent_is_a_lexer_error`,
  `test_lex_dot_without_exponent_is_unchanged`.
- `crates/crush-lang-sdk/tests/float_exponent_literals.rs`:
  `scheduler_prints_exponent_floats`, `portable_vm_prints_exponent_floats`,
  `fastvm::fastvm_prints_exponent_floats`, `emit_formats_carry_exponent_floats`
  (vm, casm, cvms, cast artifacts run by crush-run, plus the ast view re-compiled),
  `malformed_exponent_is_loud_in_crushc`.
- `crates/tree-sitter-crush/test/corpus/slices_and_radix.txt`: "Float Literals With A
  Decimal Exponent".

**Not verified here.** The AOT C and Rust backends were not run on these literals.
They consume the same CASM/CAST float value, so the lexer change does not reach
them, but this was not executed.

**Left open.** Nothing filed. Print format of non-integral floats (above) is a
candidate for its own ticket.

