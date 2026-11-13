# CRUSH-493 — Frozen-format compat corpus: committed .castb/.casm/.cvm1 files that must keep loading

| Field | Value |
|-------|-------|
| **ID** | CRUSH-493 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#493](https://github.com/nixpt/crush-ast/issues/493) |
| **Size** | M |
| **Dependencies** | CRUSH-457, CRUSH-458 |

## Problem

The freeze promises old files keep loading, but no committed corpus of format fixtures enforces it. RC F5 is failing (RELEASE-1.0-CRITERIA.md:95): two .cvm1 files sit in examples/crush/ (arrays_and_loops.cvm1, fibonacci.cvm1) and no test references them. A v2 blob is built by hand only in crush-vm/src/tests/kind_table.rs:29-33. The only repo-wide format check (crates/crush-cast/tests/repo_cast_files.rs) covers .cast.json only. Once the .castb header lands, a regression in the legacy reader or a CASM/CVM1 version-gate change would go unnoticed.

## Evidence (at origin/main 7ecd2515)

- docs/planning/RELEASE-1.0-CRITERIA.md:95 (F5)
- examples/crush/arrays_and_loops.cvm1, examples/crush/fibonacci.cvm1 (unreferenced)
- crates/crush-cast/tests/repo_cast_files.rs
- crates/casm/tests/ver02_version_gate.rs
- crates/crush-vm/src/bytecode.rs:1-22

## Goal

tests/compat/v0.12/ holds one tiny program in each format: .cast.json, headerless .castb, headered .castb, .casm, .casmb (positional and named), .cvm1 v2, and .cvm1 v3 with a kind table (carrying cosm_version). A test loads each and asserts the decoded program equals a golden. Each .cvm1 fixture, plus the two examples/crush/*.cvm1 files, is also run on the scheduler VM with stdout compared to a golden. A regen script exists and is NOT run in CI. Fixtures stay under ~50 KB total.

## Non-goals

- Cross-engine execution equivalence
- Fuzzing
- Fixtures for releases before 0.12

## Files

- crates/crush-lang-sdk/tests/compat_corpus.rs (new)
- tests/compat/v0.12/ (new fixtures + goldens)
- scripts/regen-compat-corpus.sh (new)

## Done when

- cargo test -p crush-lang-sdk --test compat_corpus passes, loading every fixture and running every .cvm1 (including the examples/crush ones) on the scheduler VM against golden stdout
- corrupting one fixture byte makes the test fail
- du -sb tests/compat < 51200
- cargo +1.100.0 clippy -p crush-lang-sdk --all-targets -- -D warnings clean

## Depends on

- CRUSH-457
- CRUSH-458

## Refs

- RC gate F5

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
