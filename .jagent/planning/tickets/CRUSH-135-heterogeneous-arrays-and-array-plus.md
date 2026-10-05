# CRUSH-135 — Arrays: heterogeneous literals rejected; `arr + [x]` rejected though `push` works

| Field | Value |
|-------|-------|
| **ID** | CRUSH-135 |
| **Priority** | P3 |
| **Status** | Done (2026-10-05, branch `claude/crush-135-array-any-concat`) |
| **Phase** | M1 |
| **Assignee** | claude |
| **Dependencies** | none |
| **Estimated effort** | S |
| **GitHub** | [#75](https://github.com/nixpt/crush-ast/issues/75) (filed 2026-10-04 by pranix) |

## Problem

`["hello", 0]` → "Array elements must have compatible types"; `[1,2] + [3]` → "Invalid binary op +". Maps already allow mixed values. Language-design decision: union/`any` element type, and whether `+` concatenates arrays.

## Reproduction

From [#75](https://github.com/nixpt/crush-ast/issues/75) (verbatim):

#### Summary

```crush
let a = ["hello", 0]
```

`[type] Array elements must have compatible types`. Map literals allow mixed value types, so the array restriction is surprising.

Related inconsistency:

```crush
let a = [1, 2]
let b = a + [3]
```

`Invalid binary op + for array<int>` — but `a.push(3)` works fine.

#### Expected

Either support heterogeneous arrays (union element type), or document the restriction and make `+` consistent with `push`. The current state — literals rejected, `+` rejected, `push` accepted — is hard to predict.

## Success criteria

- [x] decision recorded (dejavue)
- [x] behaviour matches it on all backends, with tests

## Technical approach

- Infer `array<any>` for mixed literals; add array `+` as concat if accepted.

## Files to modify

- `crates/crush-frontend/src/semantics.rs`
- `crates/crush-frontend/src/compiler.rs`
- VM `ADD`

## Decision (owner interview, 2026-10-05)

- Mixed array literals type as `array<any>`; uniform literals keep their precise element type.
- `a + b` on two arrays returns a new array (a's elements then b's); neither operand changes. Needs an array-concat path on every backend.
- Recorded in `.dejavue/decisions.md` (2026-10-05).

## Resolution

- **Type checker:** a mixed array literal is `array<any>` (uniform ones keep their element type). `array + array` is an array whose element type is the merge of both sides, or `any`. `array + int` is still a compile error; `array + any` is checked at run time.
- **Runtime `+` on two arrays** builds a new array (a's elements then b's). Neither operand changes. Implemented on every backend:
  - CVM1: `arithmetic::add_values`, shared by the scheduler and PortableVm. PortableVm's non-numeric guard lets two arrays through under ADD.
  - FastVM: `concat_arrays`.
  - JIT: an array branch in `OP_ADD_STR`. A non-numeric operand (`[1] + 2`, `null + 1`) is now a type error there too; it used to read as `0.0`.
  - AOT Rust: `bin_add`.
  - AOT C: `_add`, which errors loudly on pool/capacity exhaustion. A non-numeric operand is now a type error instead of garbage.
- **Tests:**
  - `crush-lang-sdk/tests/gh_issue_75_mixed_arrays_and_concat.rs`: the issue's repros, non-mutation, and `array + number` still rejected.
  - `differential_aot.rs`: `aot_array_concat_and_mixed_literals` and `aot_array_plus_number_rejected`, strict on every backend including the JIT.
  - Each fails without its fix.
- **Found on the way:** CRUSH-146, the JIT's `arr_set` stack contract. Fixed here.
