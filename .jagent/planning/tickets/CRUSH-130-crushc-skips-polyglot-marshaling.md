# CRUSH-130 — `crushc → .cvm1` skips the polyglot marshaling pass

| Field | Value |
|-------|-------|
| **ID** | CRUSH-130 |
| **Priority** | P2 |
| **Status** | Done (2026-10-05, branch `claude/polyglot-fail-loud`) |
| **Phase** | M4 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **GitHub** | [#70](https://github.com/nixpt/crush-ast/issues/70) (filed 2026-10-04 by pranix) |

## Problem

`prepare_polyglot_blocks` runs only inside `crush_lang_sdk::compile::compile_crush_source`; `crushc` (`crates/crush-lang-sdk/src/bin/crushc.rs`) compiles via `Compiler::compile` + `casm_to_vm` directly, so `@lang` blocks lose variable marshaling and write-back. Confirmed by reading; not run here.

## Reproduction

From [#70](https://github.com/nixpt/crush-ast/issues/70) (verbatim):

#### Summary

`crush-run run prog.crush --polyglot` correctly marshals variables into/out of `@lang` blocks, but `crushc prog.crush -o prog.cvm1` followed by `crush-run run prog.cvm1 --polyglot` does not — the guest blocks see no variables and produce no write-back.

#### Cause

`prepare_polyglot_blocks` is only wired into `crush_lang_sdk::compile::compile_crush_source` (the `crush-run run prog.crush` path). The `crushc` emit path never runs it.

#### Expected

Both compilation paths should run the polyglot preparation pass, or `crushc` should error when it encounters `@lang` blocks it cannot marshal.

## Success criteria

- [x] the same `@python` program behaves identically via `crush-run run x.crush` and `crushc x.crush -o x.cvm1 && crush-run run x.cvm1`

## Technical approach

- Route crushc through one shared entry point that runs the pass (or call it from crushc).

## Files to modify

- `crates/crush-lang-sdk/src/bin/crushc.rs`
- `crates/crush-lang-sdk/src/compile.rs`

## Resolution

Reproduced on the branch base first — worse than reported: `crushc` rejected the program at type-check (`Undefined variable: result`), because the pass is also what declares a block's output variable. `prepare_polyglot_blocks` is now public and `crushc` runs it right after parsing. Root cause was wider: `crush_frontend::compile_crush_source` can't run the pass (it needs the Python analyzer — a dependency cycle), and `crush-aotc` and `crush-pkg check` called it directly too; they now go through the new `crush_lang_sdk::compile::compile_crush_to_casm`, which `compile_crush_source` also uses.

Live: `crushc p.crush -o p.cvm1 && crush-run run p.cvm1 --polyglot` → `r=10` (with and without `--optimize`), same as `crush-run run p.crush`. Test: `crush-lang-sdk/tests/crushc_polyglot.rs` (fails without the fix).

Not changed here (filed as CRUSH-141): crushc still skips `cast_enrich` and runs the optimizer only with `--optimize`, unlike `crush-run`.
