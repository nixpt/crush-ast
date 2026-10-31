# CRUSH-279 — `Compiler::compile` silently dropped the walker COSM module

| Field | Value |
|-------|-------|
| **ID** | CRUSH-279 |
| **Priority** | P2 |
| **Status** | Done — fix commit `c9f349f` (CRUSH-277 S5) merged in PR #285 (2026-10-21) |
| **Phase** | COSM K3 — frontend transport |
| **Assignee** | unassigned |
| **Filed by** | muse — 2026-10-20, found by CRUSH-277 S5's corpus run |

## Problem

CRUSH-277 S1 added the transport: a walker puts its COSM module on
`crush_cast::Program::cosm`, and the frontend merges it. The merge
lived only in `crush_frontend::compile_cast_with_cosm`. The public
`crush_frontend::compiler::Compiler::compile` — the entry point the
node-corpus harness (and any embedder using the `Compiler` struct)
calls — ran `cosm_lower::lower` and ignored `program.cosm` entirely.

Effect: boa-compiled class programs lowered fine but failed at
CAST→CASM with "`new P(…)` constructs a kind the program doesn't
declare" (or, with no `new`, silently lost their kind table): the
class declarations vanished between the walker and the compiler.

## Fix

`Compiler::compile` now takes `program.cosm`, lowers the surface
syntax, and merges the two with the same
`cosm_lower::merge_walker_module` step `compile_cast_with_cosm`
performs (a name collision between the two is still an error).
Cosm-less programs are unchanged.

Evidence: node corpus `class_basic`/`class_extends` run node-exact
through the harness's `Compiler::compile` path; the corpus's class
area on boa is 5 match / 0 loud / 0 silent (CRUSH-277 S5).

## Survey 2026-10-31 (status)

Commit `c9f349f` (its message names CRUSH-279) is an ancestor of origin/main via PR #285. Previous status: `` Fixed on `agent/muse/CRUSH-277-s5` — awaiting merge ``.
