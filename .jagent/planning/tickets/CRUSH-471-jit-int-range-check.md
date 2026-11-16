# CRUSH-471 — JIT: ints past 48 bits refuse loudly, never truncate (H5)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-471 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#471](https://github.com/nixpt/crush-ast/issues/471) |
| **Size** | M |
| **Dependencies** | CRUSH-454 |

## Problem

JitValue::int checks the 48-bit payload range only with debug_assert! (crates/crush-jit/src/value.rs:38-43); in release an out-of-range i64 is silently masked to a different value. Non-test call sites that can receive arbitrary i64s: runtime.rs:189 (capability results), :806-813 (int conversion from float casts and s.parse::<i64>()), lib.rs:270 (host result on resume). Where overflow is detected (runtime.rs:1312-1317) the JIT sets ctx.error = 1, rendered by error_message (lib.rs:47-75) as 'JIT execution error (flag=1)', not a named limit. H5 / checkpoint 1: by 0.13 the JIT refuses loudly instead of truncating; deopt comes after 1.0.

## Evidence (at origin/main 7ecd2515)

- crates/crush-jit/src/value.rs:28-46
- crates/crush-jit/src/runtime.rs:145, 189, 806-813, 1312-1317
- crates/crush-jit/src/lib.rs:47-75, 270
- checkpoint 1 (H5 timing)

## Goal

No JIT path can produce a truncated int. Every non-test JitValue::int site with an unbounded input goes through try_int. A valid i64 beyond the JIT's 48-bit payload yields a distinct named error ('value exceeds the JIT's 48-bit int range; run on FastVM'); genuine i64 overflow still maps to ERR_ARITH_OVERFLOW / 'arithmetic overflow' as on CVM1. The unchecked constructor becomes crate-private or test-only.

## Non-goals

- Deoptimizing to the VM (after 1.0)
- Widening JIT ints
- Float pow helper
- JIT on/off default (CRUSH-JIT-OFF-BY-DEFAULT)

## Files

- crates/crush-jit/src/value.rs
- crates/crush-jit/src/runtime.rs
- crates/crush-jit/src/lib.rs
- crates/crush-jit/tests/int_range.rs (new)

## Done when

- cargo test -p crush-jit --test int_range passes: for add/sub/mul/neg, int() conversion from a string and from a float, and a capability returning 2^47, each case returns the named 48-bit refusal (text asserted) or, for true i64 overflow, 'arithmetic overflow'; no case returns a wrapped value; each test uses assert_jit_compiled (compilation_count() >= 1)
- a unit test calls the conversion helpers directly with INT_MAX+1 and INT_MIN-1, proving the path does not rely on debug_assert (no release build needed)
- the PR lists `rg -n 'JitValue::int\(' crates/crush-jit/src` with a justification per remaining unchecked site
- cargo +1.100.0 clippy -p crush-jit --all-targets -- -D warnings is clean

## Depends on

- CRUSH-454

## Refs

- H5
- ruling 18
- CRUSH-142
- CRUSH-213

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
