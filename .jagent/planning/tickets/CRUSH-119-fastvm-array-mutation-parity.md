# CRUSH-119 — FastVM array mutation return-value parity

| Field | Value |
|-------|-------|
| **ID** | CRUSH-119 |
| **Priority** | P2 |
| **Status** | Done — merged in `4858bd6` / v0.3.6 |
| **Phase** | M1 |
| **Assignee** | Buffy |
| **Dependencies** | CRUSH-7 |
| **Estimated effort** | S |

## Problem

The FastVM execution path did not preserve mutated array values consistently
enough for real loop-carried accumulator programs. This left array mutation
partially fixed in CVM1/PortableVM while the faster execution tier still
diverged on the same source shape.

## Resolution

Implemented in `55ec59a` and merged through `4858bd6`. FastVM now preserves
the mutated array value for the covered `push`/`append`/`arr_set` paths, and
`crush-lang-sdk` gained source-pipeline regressions covering native array
mutation contracts and compiled range/array-loop behavior.

This closes the FastVM loop/regression part of CRUSH-7. CRUSH-7 itself remains
partial because nested array indexing and slice syntax are still separate
residual gaps.

## Verification

```text
CARGO_TARGET_DIR=/tmp/target-crush-ast-foreman-review CARGO_BUILD_JOBS=2 cargo check -p crush-vm -p crush-lang-sdk -p crush-frontend -p crush-aot -p crush-aotc
passed

cargo test -p crush-frontend --lib
83 passed

cargo test -p crush-lang-sdk --lib
183 passed
```
