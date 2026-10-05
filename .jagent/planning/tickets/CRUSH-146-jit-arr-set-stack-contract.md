# CRUSH-146 — JIT `arr_set` didn't push the array back: `a[i] = v` corrupted the stack

| Field | Value |
|-------|-------|
| **ID** | CRUSH-146 |
| **Priority** | P1 |
| **Status** | Done (2026-10-05, branch `claude/crush-135-array-any-concat`) |
| **Phase** | M1 |
| **Assignee** | claude |
| **Dependencies** | none |
| **Estimated effort** | XS |

## Problem

Found while testing CRUSH-135 on every backend. `a[i] = v` compiles to `<a>; <i>; <v>; CAP_CALL "arr_set" 3; POP`. CVM1 and FastVM push the array back and the compiler POPs it. The JIT's `OP_ARR_SET` pushed nothing, so the POP ate a live value:

```crush
fn main() { let b = [1, 2] b[0] = 50 return b[0] }
```

CVM1/FastVM/AOT → 50; JIT → null. In a loop the JIT hung or overflowed its stack. It is the same bug class as CRUSH-145 (SET_FIELD).

## Resolution

- `OP_ARR_SET` pushes the container back.
- Test: `differential_aot.rs` `aot_index_assignment_keeps_the_stack_balanced` (index writes in a loop, strict on every backend). It fails without the fix.
