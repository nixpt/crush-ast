# CRUSH-124 — `"\r"` in a string literal becomes the letter `r` on CVM1

| Field | Value |
|-------|-------|
| **ID** | CRUSH-124 |
| **Priority** | P2 |
| **Status** | Done (2026-10-04, branch `claude/gh-64-71-quick-fixes`) |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | XS |
| **GitHub** | [#64](https://github.com/nixpt/crush-ast/issues/64) (filed 2026-10-04 by pranix) |

## Problem

The frontend lexer decodes `\r` correctly, but CVM1 text assembly re-escapes and re-decodes string operands. The assembler's unescape (`crates/crush-vm/src/assembler.rs` `parse_quoted`, ~l.651) only knows `\n \t \" \\`; any other escape falls through to the bare letter. CRUSH-20's notes already observed this escape set is narrower than what the emitter produces.

## Reproduction

From [#64](https://github.com/nixpt/crush-ast/issues/64) (verbatim):

#### Summary

The `"\r"` escape sequence in a string literal produces the letter `r` (0x72), not a carriage return (0x0D). `"\n"` and `"\t"` work correctly.

#### Repro

```crush
fn main() {
  let r = "\r"
  if r == "r" { print("BUG: \\r is letter r") } else { print("ok") }
}
```

Output: `BUG: \r is letter r`.

#### Impact

Any code that treats `\r` as whitespace (e.g. `" \t\n\r"` in a `char_in` check) silently treats the letter `r` as whitespace instead, corrupting parsing. Found while writing a CAISON parser.

#### Expected

`"\r"` should lex to a single carriage-return character (0x0D), like `"\n"` → 0x0A.

## Success criteria

- [x] `"\r"` evaluates to U+000D on `crush-run run x.crush`
- [x] every escape the CASM emitter can produce round-trips (`\r`, `\0`, `\u{..}`, `\'`)
- [x] regression test in crush-vm assembler tests + a source-level test

## Technical approach

- Make `parse_quoted` the exact inverse of the emitter's escaping (Rust `escape_debug`-style: add `\r`, `\0`, `\'`, `\u{XXXX}`).

## Files to modify

- `crates/crush-vm/src/assembler.rs`

## Resolution

Reproduced on `main` `a8247af` first (RULES §1). `crush-vm/src/assembler.rs` `parse_string` now decodes the full `{:?}` escape set (`\r`, `\0`, `\'`, `\u{..}`) and rejects a malformed `\u{..}`. Unit round-trip test over Debug-quoted samples + source-level test (`crush-lang-sdk/tests/gh_issues_64_71.rs`). Live: the #64 repro prints `ok`.
