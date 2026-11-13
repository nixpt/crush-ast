# CRUSH-501 — C walker switch: fallthrough, continue, nested break and swallowed case-body errors (#329)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-501 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#501](https://github.com/nixpt/crush-ast/issues/501) |
| **Size** | M |
| **Dependencies** | none |

## Problem

The C walker desugars `switch` into an if/else chain with 'no fallthrough support' (crates/crush-lang-c/src/lib.rs:362-406), silently wrong in four ways. (1) Every case acts as if it ended in break: #329's program prints `abcd` where gcc prints `abbcd`, exit 0. (2) case_body.retain(...) (394-396, 403-405) also strips top-level `continue`, which in C belongs to the enclosing loop. (3) Only top-level breaks are stripped, so `case 1: if (c) break; ...` leaves a break that exits the enclosing loop, not the switch. (4) Case and default children are visited with `if let Ok(stmts) = self.visit_statement(case_child)` (386-391, 400-402), so any unsupported statement inside a case is dropped without an error, while the same statement elsewhere bails (511-514, CRUSH-201).

## Evidence (at origin/main 7ecd2515)

- crates/crush-lang-c/src/lib.rs:362-406, 386-391, 394-396, 400-405, 511-514
- issue #329: gcc `abbcd` vs walker `abcd`
- crates/crush-lang-c/tests/gcc_corpus_test.rs

## Goal

Lower switch with C semantics: a case without break falls through into the next body, `break` anywhere inside the switch body (including nested in if/blocks, not inside an inner loop) leaves the switch, and `continue` passes through to the enclosing loop. Anything the lowering cannot express exactly (case labels inside nested blocks, Duff's device, `continue` with no enclosing loop) is refused with a 'not supported' error. Case-body visit errors propagate with `?`, and the value expression is skipped by node id or field rather than by kind.

## Non-goals

- C globals (#330)
- main's return value (#331; not ticketed, see dropped)
- Duff's device / case labels in nested blocks (refuse)
- Jump-table optimisation

## Files

- crates/crush-lang-c/src/lib.rs
- crates/crush-lang-c/tests/gcc_corpus/

## Done when

- tests/gcc_corpus gains switch_fallthrough.c (#329's program), switch_continue.c and switch_nested_break.c; cargo test -p crush-lang-c --test gcc_corpus_test passes, so the scheduler VM output equals gcc's committed .out
- unit test: a switch whose case contains an unsupported statement (e.g. `goto`) returns Err naming it
- unit test: a switch where every case breaks still lowers and runs as before (existing corpus stays green)
- cargo +1.100.0 clippy -p crush-lang-c --all-targets -- -D warnings clean

## Depends on

- none

## Refs

- #329
- CRUSH-201
- CRUSH-238

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
