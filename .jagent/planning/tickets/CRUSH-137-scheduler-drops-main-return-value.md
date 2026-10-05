# CRUSH-137 — The scheduler (CVM1 interpreter) drops `main`'s return value

| Field | Value |
|-------|-------|
| **ID** | CRUSH-137 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

For `fn main() { return 7 }` the two CVM1 VMs disagree on where the return value goes: `PortableVm` leaves it on the stack (`VmResult.stack == [7]`), while the scheduler's `RET` with no return address does `StepAction::Done(stack.pop())` and the value never reaches `VmResult.stack`. So `crush_lang_sdk::differential` reports the interpreter's return as `None` for every program, and `assert_fastvm_agrees` silently skips the interpreter-vs-FastVM return-value comparison (it only compares when both sides are `Some`). Found while writing CRUSH-126's tests.

## Reproduction

```rust
let r = crush_lang_sdk::differential::differential_run("fn main() { return 7 }").unwrap();
// fast=Finished(Some(Int(7)))  interp=Ok { output: "", stack: [] }  port=Ok { output: "", stack: [Int(7)] }
```

## Success criteria

- [ ] the scheduler and `PortableVm` report `main`'s return value the same way
- [ ] `assert_fastvm_agrees` compares the interpreter's return value (no silent skip), and the existing differential suite still passes

## Technical approach

- Decide the contract (return value on `VmResult.stack`, or a dedicated `return_value` field) and make both VMs follow it; `GreenThread.return_value` already exists.
- Then tighten `assert_fastvm_agrees` to fail when one side has a return value and the other doesn't.

## Files to modify

- `crates/crush-vm/src/scheduler.rs` — `RET` / `StepAction::Done` handling
- `crates/crush-lang-sdk/src/differential.rs`
- `crates/crush-aot/tests/differential_aot.rs` — `assert_fastvm_agrees`
