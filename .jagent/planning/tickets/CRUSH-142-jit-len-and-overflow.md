# CRUSH-142 — JIT: `len` of an array is null; integer overflow returns 0

| Field | Value |
|-------|-------|
| **ID** | CRUSH-142 |
| **Priority** | P2 |
| **Status** | Done (2026-10-05, branch `claude/jit-len-overflow`) |
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

- [x] both return what FastVM returns
- [x] the differential harness fails (not warns) on JIT divergence, once the JIT is at parity

## Files to modify

- `crates/crush-jit/src/compiler.rs` (Len / Add), `crates/crush-jit/src/runtime.rs`
- `crates/crush-aot/tests/differential_aot.rs`

## Resolution

- **`len([1, 2, 3])` was null.** The JIT's `array_push` helper dropped the array, but FastVM and CVM1 push it back. An array literal is `new_array` followed by `push x; array_push` for each element, so every element after the first had no array to go into. `array_pop` likewise didn't leave the array. Both helpers now follow FastVM's stack contract.
- **Overflow returned 0.** The real cause was that JIT ints were only 16 bits wide. Every value outside ±32767 was truncated, so `i64::MAX` became -1, and -1 + 1 = 0. `1000 * 1000` also failed on the JIT. Ints now use the whole 48-bit NaN-box payload:
  - packing, unpacking and the inline arithmetic overflow checks all work at 48 bits;
  - `mul` additionally checks the high half of the product;
  - a constant that doesn't fit makes the JIT refuse the program, so it falls back to FastVM;
  - `OP_ADD_STR` errors instead of wrapping.

  Remaining limit: values between 2^47 and 2^63 raise an error on the JIT (FastVM computes them). They never produce a wrong value.
- **Found along the way:** `array.pop(a)`, as lowered for CRUSH-122, left the array on the stack. It is now `array_pop; swap; pop`.
- **Harness:** the differential harness is now strict on the JIT (the second success criterion). The one remaining JIT gap is recursive calls (CRUSH-87). Its test opts out by name through `assert_all_backends_agree_except_jit`.
