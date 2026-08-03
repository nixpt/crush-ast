# CRUSH-82 — Lexer redesign: byte-span tokens + string interner

| Field | Value |
|-------|-------|
| **ID** | CRUSH-82 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | Design/perf (s412) |

## Problem

Panini capture (2026-08-02): the lexer copies the whole source into a
`Vec<char>`, allocates a `String` per token, and materializes comments only to
discard them (`crush-frontend/src/parser/lexer.rs:252`; re-verify). Allocation
churn scales with source size — a design-level cost on every compile, felt by
every frontend and by CRUSH-83's cache hashing.

## Approach

Tokens carry byte spans (`start..end` into the source `&str`) instead of owned
Strings; identifier/string values resolve through an interner; comments
skipped without materialization. Keep the token-kind surface identical so the
parser change is mechanical. **Order: land AFTER CRUSH-75** (small lambda/lex
fixes) to avoid rebasing that fix across a redesign.

## Definition of done

- [ ] No per-token String allocation on the happy path (assert via a
      micro-bench or allocation counter); whole-source Vec<char> gone
- [ ] `cargo test --workspace` green; conformance corpus (CRUSH-73) green
- [ ] Lex bench before/after recorded (large fixture)

## Files in scope

- `crates/crush-frontend/src/parser/lexer.rs` + parser call sites

## Gates

After CRUSH-75. Feeds CRUSH-83.
