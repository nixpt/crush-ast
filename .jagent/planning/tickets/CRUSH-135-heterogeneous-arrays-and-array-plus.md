# CRUSH-135 — Arrays: heterogeneous literals rejected; `arr + [x]` rejected though `push` works

| Field | Value |
|-------|-------|
| **ID** | CRUSH-135 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
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

- [ ] decision recorded (dejavue)
- [ ] behaviour matches it on all backends, with tests

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
