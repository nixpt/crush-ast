# CRUSH-454 — JitEngine fallback to FastVM is observable, and JIT tests assert they compiled

| Field | Value |
|-------|-------|
| **ID** | CRUSH-454 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#454](https://github.com/nixpt/crush-ast/issues/454) |
| **Size** | M |
| **Dependencies** | none |

## Problem

JitEngine::run_with_ctx falls back to FastVM on any CompileError (crates/crush-jit/src/lib.rs:163-170 -> fallback_to_fastvm :142-147, CRUSH-72) and returns the FastVM result with no flag, so a 'JIT' test passes without the JIT compiling anything. compilation_count() exists (lib.rs:136-139) but is asserted in about 6 places, while JitEngine is exercised by about 92 tests in crush-jit/src/lib.rs plus crush-aot tests and crush-lang-sdk/src/differential.rs. A standing direction requires every JIT test to assert the JIT compiled.

## Evidence (at origin/main 7ecd2515)

- crates/crush-jit/src/lib.rs:136-139, 142-147, 163-170
- rg -c '#\[test\]' crates/crush-jit/src/lib.rs = 92; compilation_count() asserted at 6 sites
- crates/crush-aot/tests/{backend_agreement,differential_aot,stdout_parity,cosm_k1_lifted_methods,jit_exec_lang}.rs construct JitEngine
- CRUSH-72 introduced the fallback

## Goal

JitEngine exposes whether the last run fell back (e.g. last_run_fell_back() or a counter) and logs it at debug level. A shared test helper assert_jit_compiled is used by every JIT test that is not explicitly a fallback test. The differential harness reports a JIT fallback as an abstention, not as a JIT agreement.

## Non-goals

- Adding JIT opcodes
- Changing whether the JIT falls back
- JIT deopt (after 1.0)

## Files

- crates/crush-jit/src/lib.rs
- crates/crush-aot/tests/
- crates/crush-lang-sdk/src/differential.rs

## Done when

- cargo test -p crush-jit passes; every #[test] that builds a JitEngine either calls assert_jit_compiled or carries a `// fallback test` marker, enforced by a test that scans the source
- a new test shows a program using ExecLang (refused by compiler.rs) reports fallback = true and compilation_count() == 0
- cargo test -p crush-aot --test backend_agreement --test stdout_parity passes with the helper applied
- cargo +1.100.0 clippy -p crush-jit -p crush-aot -p crush-lang-sdk --all-targets -- -D warnings is clean

## Depends on

- none

## Refs

- CRUSH-72

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
