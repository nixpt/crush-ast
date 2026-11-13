# CRUSH-488 — Tier-1 ArithmeticOverflow matrix (ruling 46: i64 with checked overflow)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-488 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#488](https://github.com/nixpt/crush-ast/issues/488) |
| **Size** | M |
| **Dependencies** | CRUSH-453, #425 |

## Problem

Ruling 46 fixes i64 with checked overflow. The optimizer folds only with checked_* and leaves overflow to the runtime (optimizer.rs:369-377). The VM arithmetic uses checked ops (arithmetic.rs:77,89,101, 226, 264) and FastVM has its own copies (fastvm/arithmetic.rs:151,160). The only cross-engine overflow test (differential_aot.rs:452) covers i64::MAX + 1 and compares only ok/err. Nothing asserts each Tier-1 engine reports ArithmeticOverflow for the other cases.

## Evidence (at origin/main 7ecd2515)

- crates/crush-frontend/src/optimizer.rs:369-377
- crates/crush-vm/src/arithmetic.rs:77,89,101,182-195,226,264
- crates/crush-vm/src/fastvm/arithmetic.rs:151,160
- crates/crush-vm/src/fastvm/execution.rs:54-58
- crates/crush-aot/tests/differential_aot.rs:452-456
- ruling 46

## Goal

A matrix through the tier1 harness of 8 cases: i64::MAX+1, i64::MIN-1, i64::MAX*2, -i64::MIN, i64::MIN / -1, i64::MIN % -1, math.power(2, 63), and a runtime (non-folded) a*b. Each runs on 3 engines with operands passed through `any`-typed functions so the optimizer cannot fold them. Measure first; fix any engine that wraps, panics, returns a float or errors with a different class. Where an engine lacks math.power, record it as a disagreement and fix it or file it.

## Non-goals

- Bigint
- The JIT 48-bit refusal (CRUSH-JIT-INT-RANGE-CHECK)
- Shell walkers (#332)
- MATH_POW opcode semantics (float by design)
- The bitwise/remainder cases owned by #425

## Files

- crates/crush-lang-sdk/tests/tier1_int_overflow.rs (new)
- crates/crush-vm/src/fastvm/arithmetic.rs
- crates/crush-vm/src/fastvm/execution.rs

## Done when

- tier1_int_overflow: 24 assertions (8 cases x 3 engines) pass, all ErrorClass::ArithmeticOverflow
- the same cases wrapped in try/catch are caught on all three engines
- the test runs under the default debug profile with no panic
- cargo +1.100.0 clippy -p crush-vm -p crush-lang-sdk --all-targets -- -D warnings is clean

## Depends on

- CRUSH-453
- #425

## Refs

- ruling 46
- CRUSH-189
- #425

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
