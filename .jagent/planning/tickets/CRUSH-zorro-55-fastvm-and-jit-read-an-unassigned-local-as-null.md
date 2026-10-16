# CRUSH-zorro-55 — FastVM and the JIT read an unassigned local as null

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-55 |
| **Priority** | P1 |
| **Status** | Done |
| **Phase** | M2 |
| **Assignee** | claude (vega) |
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

- [x] FastVM and the JIT fail on an unassigned read with `load from uninitialised slot`.
- [x] `STILL_SILENT` in `backend_agreement.rs` is empty.

## Resolution

**Measured first** (`crates/crush-aot/tests/backend_agreement.rs`, each case a
CASM read the frontend would refuse, made by sending a store to a dead local):

| case | sched/portable VM | AOT Rust / C | FastVM before | JIT before |
|---|---|---|---|---|
| read in `main`, never assigned | loud | loud | `Null` | `Null` |
| assigned only on the untaken branch | loud | loud | `Null` | `Null` |
| loop assigns before reading | 12 | 12 | 12 | 12 |
| callee local read before its `let` | loud | loud | `TypeMismatch` (null + int) | `flag=1` (null + int) |
| `g(true) + g(false)`, same frame | loud | loud | `TypeMismatch` | **14** — the first call's 7 |
| recursion, second pass reads unassigned | loud | loud | `TypeMismatch` | **92** — deeper frames' values |

After: every engine fails with `load from uninitialised slot` on every loud
case, and agrees on 12 / 92 where the read is assigned. The JIT outcome in the
harness now fails if `JitEngine::run` fell back to FastVM, so it measures the
JIT.

**Shared pass.** `crates/crush-aot/src/assigned.rs` moved to
`crates/casm/src/assigned.rs` (`casm::assigned::loads_to_check`); AOT Rust,
AOT C, crush-aotc and `crush_vm::fastvm::lower_program` all call it. It now
also follows FastVM's op aliases, string label targets, `break`/`continue` and
`enter_try`'s handler edge; an unresolvable target checks every load.

- **FastVM:** a store past the end of the flat locals vector pads the gap with
  `UNASSIGNED_LOCAL` (`RuntimeValue::Ref(usize::MAX)` — no arena index can be
  that) instead of `Null`; a slot past the end is unassigned by construction,
  and a return truncates the frame away, so a new frame never sees old values.
  Unproven loads lower to the new `FastOp::LoadLocalChecked` (added at the end
  of the enum), which fails with `ExecutionError("load from uninitialised slot
  N")`; proven loads keep `LoadLocal`. Chosen over a parallel bitset (state to
  thread through `execute_one`, frames and the collector, and a second write on
  every store) and a new `RuntimeValue` variant (a non-value in every match).
  CVM1 bytecode (`lower_bytecode_program`) has no pass: all its loads are
  checked.
- **JIT:** frame locals are fixed memory reused by the next call at the same
  depth. Each function entry — the `Call` site after `push_frame`, and `main`
  on initial (not resumed) entry — stores `TAG_UNASSIGNED`
  (`0x7FFC_0000_0000_0003`, reserved in the null/bool namespace) into exactly
  the slots its checked loads read. `LoadLocalChecked` compares and branches to
  a cold path that calls the new `OP_UNINIT_LOAD` helper (message +
  `ctx.error = ERR_UNINIT`), stores a null result and returns; the engine maps
  `ERR_UNINIT` to the message. A checked slot >= `FRAME_LOCALS` (8) would alias
  the next frame, so such a program is refused (`CompileError::Unsupported`)
  and falls back to FastVM, which is loud.
- **crush-aotc:** locals with a checked load are kept boxed (`CrushValue`) and
  start as `CV_UNSET` (`0x7FFC000000000003`) instead of `= 0` — an unboxed
  `int64_t`/`double` has no spare bit pattern — and that load exits with
  "crush(aotc): load from uninitialised slot `x`", as AOT C does.

**Cost:** 0 of 2054 FastVM loads in `examples/crush` (41 programs) are
checked, so FastVM and JIT code for them is unchanged.

**Harness lists:** `js_parity.rs`, `python_parity.rs` and
`node_corpus_test.rs` don't run FastVM or the JIT; nothing flipped.

**Left as is:** a JIT function using more than 8 locals still overlaps the next
frame's slots for unchecked loads/stores — pre-existing, separate from this
ticket.
