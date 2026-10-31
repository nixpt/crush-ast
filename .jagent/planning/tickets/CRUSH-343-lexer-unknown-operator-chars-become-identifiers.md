# CRUSH-343 — Lexer: an unknown operator character becomes an identifier

| Field | Value |
|-------|-------|
| **ID** | CRUSH-343 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | Correctness spine |
| **Split from** | CRUSH-75 (whose lambda half is done) |

## Problem

`crush-frontend`'s lexer turns a character it has no token for into `Token::Ident` — e.g. a lone `&`
(`crates/crush-frontend/src/parser/lexer.rs:1123`: `// Single & as ident for now`). So a typo or an
unsupported operator becomes a confusing *semantic* error instead of a lexical one:

```crush
let a = 3
let b = a & 1
print(b)
```

```text
[compile] Undefined variable: &
```

It fails loudly (not silently wrong), but the message points at the wrong problem.

## Done when

- [ ] Every character with no token is a lexer error naming it and its position, e.g.
      `unexpected character '&' at line 2, column 11` (or `&` is given a real meaning, if bitwise
      operators are wanted — a language decision, not this ticket's).
- [ ] A test per formerly-ident-ified character.
