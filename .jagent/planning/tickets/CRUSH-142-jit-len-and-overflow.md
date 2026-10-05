# CRUSH-142 — JIT: `len` of an array is null; integer overflow returns 0

| Field | Value |
|-------|-------|
| **ID** | CRUSH-142 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M2 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

With the differential harness's JIT noise fixed (a temp file shared between parallel tests), exactly two JIT divergences remain, every run:

| source | FastVM / CVM1 | JIT |
|---|---|---|
| `fn main() { return len([1, 2, 3]); }` | `3` | `null` |
| `fn add_any(a: any, b: any) { return a + b; }` / `add_any(9223372036854775807, 1)` | arithmetic-overflow error | `0` |

The harness only warns on JIT divergence, so neither fails a test.

## Success criteria

- [ ] both return what FastVM returns
- [ ] the differential harness fails (not warns) on JIT divergence, once the JIT is at parity

## Files to modify

- `crates/crush-jit/src/compiler.rs` (Len / Add), `crates/crush-jit/src/runtime.rs`
- `crates/crush-aot/tests/differential_aot.rs`
