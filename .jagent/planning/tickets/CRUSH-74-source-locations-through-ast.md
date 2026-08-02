# CRUSH-74 — Wire source locations through the AST (connect the cut span wire)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-74 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | Correctness spine (s412) |

## Problem

Both ends exist, the middle is cut: lexer tokens carry
`SourceLocation{line,col}`, and `casm/src/debug_info.rs` has `SourcePos` +
span types with `Program::with_source_map` accepted by the assembler
(July evidence: assembler.rs:147) — but the real parser constructs EVERY AST
node with `meta: HashMap::new()` (`grep -c 'insert("line"'` on
`crush-frontend/src/parser/mod.rs` = 0), and `compiler.rs` `create_instr`
(July: :1945) reads only `meta["lang"]`. Trap: `compiler_tests.rs`'s
`meta_at(line,col,file)` helper FABRICATES meta no real parse produces — its
passing tests are not evidence that location data flows. This blocks real
diagnostics, the debugger's source map (M3), and useful runtime errors.

**Re-verify at dispatch:** grep the sites above; line numbers are July-vintage.

## Approach

Connect-the-wire, not a from-scratch span project (estimated days):
1. Parser: stamp `meta["line"]`/`meta["col"]` (or a typed field if cheap) from
   the current token at each node-construction site.
2. Compiler: `create_instr` threads line/col into casm debug_info; emit
   `Program::with_source_map`.
3. Kill or quarantine `meta_at` fixtures so tests exercise the real path.
4. End-to-end test: compile a 2-function file, assert a runtime error reports
   the correct line — which requires CRUSH-79's flat-vector bug fixed at the
   casm end; coordinate or absorb it explicitly.

## Definition of done

- [ ] Real parse produces nodes with location; a test asserts it (no meta_at)
- [ ] Runtime error from a multi-function program reports correct file:line
- [ ] Debugger source-map item (M3) unblocked — note in its TASKS entry
- [ ] `cargo test --workspace` green

## Files in scope

- `crates/crush-frontend/src/parser/mod.rs`, `compiler.rs`, `crates/casm/src/debug_info.rs` (with CRUSH-79)

## Gates

Pairs with CRUSH-79 (far-end bug). Feeds M3 debugger completion.
