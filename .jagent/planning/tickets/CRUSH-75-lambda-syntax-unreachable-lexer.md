# CRUSH-75 — Lambdas unreachable from source: lexer bare-`|` shortcut + silent unknown-operator Idents

| Field | Value |
|-------|-------|
| **ID** | CRUSH-75 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | Correctness spine (s412) |

## Problem

A whole language feature is disabled by a "for now" lexer shortcut.
`Expression::Lambda` exists in crush-cast, `compiler.rs` compiles it (July:
:1588, lifting to `__lambda_N`), the tree-sitter grammar has 5 lambda rules,
and `crates/tree-sitter-crush/test_lambda.crush` documents `|a, b| { ... }` —
but the lexer (July: lexer.rs:700-710) lexes `|>` → `Token::Pipe`, `||` →
`Token::Or`, and a BARE `|` → `Token::Ident("|")` ("Single | as ident for
now"), while the parser's sole Lambda site expects `Token::Pipe`. Empirically
verified in July: `|a, b| { return a + b; }`, `|x, y| => x * y`, `|x| => x + n`
all parse-error. Two frontends disagree on the language. Worse, the lexer's
fallback turns ANY unrecognized operator char into an Ident ("Single & as
ident for now" too) — typos silently become identifiers.

**Re-verify at dispatch:** run the three repro snippets above through the
current parser first.

## Approach

1. Lex bare `|` as a real token; parse `|params| body` lambda syntax per the
   tree-sitter grammar's documented form.
2. Compiler already handles Lambda — but note it captures NOTHING from the
   enclosing scope (no closures — that's a separate, bigger gap; see the July
   research §"no closures" and file it independently if in reach). This ticket
   = syntax reachability only; a lambda using only its params must work
   end-to-end.
3. Unknown operator chars → lex error, not Ident.
4. Flip CRUSH-73's xfail lambda corpus entry to expected-green.

## Definition of done

- [ ] `test_lambda.crush`'s documented syntax parses + compiles + runs (param-only bodies)
- [ ] Capturing lambdas produce a CLEAR compile error (not silent wrong behavior), with a filed follow-up ticket for closures
- [ ] Unknown operator char → lex error with location; test
- [ ] `cargo test --workspace` green; conformance corpus updated

## Files in scope

- `crates/crush-frontend/src/parser/lexer.rs`, `parser/mod.rs`
- Coordinate: land BEFORE CRUSH-82's lexer redesign

## Gates

None. Before CRUSH-82.
