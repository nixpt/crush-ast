# CRUSH-132 — `crush-aotc --emit rust` silently drops unsupported ops (incl. `exec_lang`)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-132 |
| **Priority** | P2 |
| **Status** | Backlog |
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

- [ ] an unsupported op makes `crush-aotc` fail with an error naming the op and source location
- [ ] `@python` either works or is rejected

## Technical approach

- Turn the fallback into a codegen error; list deliberately-NOP ops explicitly.
- Decide whether unknown cap_calls should error at compile time or at run time.

## Files to modify

- `crates/crush-aot/src/codegen.rs`
