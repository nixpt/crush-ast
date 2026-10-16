# CRUSH-zorro-55 — FastVM and the JIT read an unassigned local as null

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-55 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M2 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |
| **Filed by** | claude (vega) — 2026-10-15, measured while fixing CRUSH-zorro-54 |

## Problem

A `load` of a local that no `store` has assigned yet fails on the scheduler VM
and the portable VM (`load from uninitialised slot N`) and, since
CRUSH-zorro-54, on AOT Rust and AOT C. **FastVM and the JIT still return
`null`** and carry on, so a walker that reads a local before assigning it
(e.g. a top-level JS function reading a script variable on the swc lowering)
prints a wrong answer there.

Pinned in `crates/crush-aot/tests/backend_agreement.rs` (`STILL_SILENT`):
`unassigned_local_read_*` fail on every other engine and expect `Null` from
these two. Remove the entries when this lands.

- **FastVM** (`crush-vm/src/fastvm/execution.rs`, `FastOp::LoadLocal`): locals
  are a flat `Vec<RuntimeValue>` padded with `Null` when a store extends it, so
  an unassigned slot is indistinguishable from a stored `null`.
- **JIT** (`crush-jit/src/compiler.rs`, `lload`): frame locals are fixed
  per-frame memory; nothing marks a slot unassigned (and a reused frame may
  hold a previous call's value).

The `crush-aotc` crate (standalone CASM → C, no in-tree users) declares
non-parameter locals `= 0` and has the same class.

## Fix

Same as CRUSH-zorro-54: an "unassigned" marker per local slot, and a checked
load that fails with `load from uninitialised slot`. Keep the cost low with the
definite-assignment pass in `crush-aot/src/assigned.rs` (move it to `casm` so
`fastvm::lower_program` can use it): only loads not preceded by a store on
every path need a `LoadLocalChecked`-style op.

## Success criteria

- [ ] FastVM and the JIT fail on an unassigned read with `load from uninitialised slot`.
- [ ] `STILL_SILENT` in `backend_agreement.rs` is empty.
