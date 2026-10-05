# CRUSH-132 — `crush-aotc --emit rust` silently drops unsupported ops (incl. `exec_lang`)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-132 |
| **Priority** | P2 |
| **Status** | Done (2026-10-05, branch `claude/polyglot-fail-loud`) |
| **Phase** | M2 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **GitHub** | [#72](https://github.com/nixpt/crush-ast/issues/72) (filed 2026-10-04 by pranix) |

## Problem

The Rust AOT codegen's fallback arm (`crates/crush-aot/src/codegen.rs` ~l.782, `// Unknown / NOP`) emits nothing for any op it doesn't implement — `exec_lang` is one case. Unknown `cap_call`s are likewise stubbed to `Null` (~l.774). Confirmed by reading; not run here.

## Reproduction

From [#72](https://github.com/nixpt/crush-ast/issues/72) (verbatim):

#### Summary

`crush-aotc --emit rust` on a program containing `@python` blocks compiles without error, but the guest code never runs: `exec_lang` hits the codegen catch-all no-op arm, so the block is silently dropped from the generated Rust.

#### Expected

Either codegen supports `exec_lang` for `@python` (and other `@lang` targets), or `crush-aotc` errors loudly when it encounters a block it cannot compile. Silent miscompilation is the worst option.

## Success criteria

- [x] an unsupported op makes `crush-aotc` fail with an error naming the op and source location
- [x] `@python` either works or is rejected

## Technical approach

- Turn the fallback into a codegen error; list deliberately-NOP ops explicitly.
- Decide whether unknown cap_calls should error at compile time or at run time.

## Files to modify

- `crates/crush-aot/src/codegen.rs`

## Resolution

`gen_rust_source` / `gen_c_source` now return `Result<String, crush_aot::UnsupportedOps>`; the fallback arms record what they couldn't translate (unknown ops, unknown `cap_call`s, and the C backend's `str_split`/`str_replace`/`str_join` null stubs), and generation fails naming each op, function and instruction — e.g. ``the Rust AOT backend cannot compile: `exec_lang` (fn main, instruction 3)``. `nop` stays an explicit no-op. Decision on the open question: unknown cap_calls are a compile-time error.

Impact over `examples/crush`: 17 programs per backend that used to "compile" now fail loudly (`exec_lang` ×34, `str.len`, `math.*`, `fs.*`, `try`/`throw`, …); the remaining failures there are pre-existing frontend errors. Test: `crush-aot/tests/unsupported_ops.rs`; the existing crush-aot suite passes unchanged.
