# CRUSH-120 — Optimizer preserves self-referential assignment values

| Field | Value |
|-------|-------|
| **ID** | CRUSH-120 |
| **Priority** | P2 |
| **Status** | Done — merged in `4858bd6` / v0.3.6 |
| **Phase** | M1 |
| **Assignee** | Buffy |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

The frontend optimizer could constant-propagate the right-hand side of a
self-referential assignment before invalidating the assigned variable. In loop
carried updates, that let stale constants survive past mutation and changed
program behavior.

## Resolution

Implemented in `ef3992c` and merged through `4858bd6`. Assignment invalidation
now happens before right-hand-side constant propagation, so loop-carried
accumulators and other self-referential updates preserve runtime values.

## Verification

```text
CARGO_TARGET_DIR=/tmp/target-crush-ast-foreman-review CARGO_BUILD_JOBS=2 cargo check -p crush-vm -p crush-lang-sdk -p crush-frontend -p crush-aot -p crush-aotc
passed

cargo test -p crush-frontend --lib
83 passed

cargo test -p crush-lang-sdk --lib
183 passed
```
