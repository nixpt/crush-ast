# CRUSH-268 — string slice syntax `"hello"[1:3]` unsupported

| Field | Value |
|-------|-------|
| **ID** | CRUSH-268 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M6 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

Array slice syntax `xs[1:3]` works (CRUSH-7, `8f30a96`), but slicing a string errors loudly: `"hello"[1:3]` → `[runtime] type error: expected array, got str`. The slice op assumes an array; strings aren't accepted. Flagged by vega while closing CRUSH-7 (2026-10-15) as the one remaining slice gap.

## Success criteria

- [ ] `"hello"[1:3]` → `"el"` (matching array-slice semantics).

## Technical approach

- Extend the slice opcode path to accept `Value::Str` and return a substring, matching array-slice start/end semantics.

## Files to modify

- `crates/crush-vm/src/portable_vm.rs` / `scheduler.rs` — slice opcode: accept string
- `crates/crush-aot/src/codegen*.rs` — AOT parity for string slice

## Non-goals

- Step slicing `xs[::2]`
