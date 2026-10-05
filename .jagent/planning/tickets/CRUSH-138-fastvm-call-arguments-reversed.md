# CRUSH-138 — FastVM binds call arguments in reverse order

| Field | Value |
|-------|-------|
| **ID** | CRUSH-138 |
| **Priority** | P1 |
| **Status** | Done (2026-10-05, branch `claude/fastvm-args-execlang`) |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S–M |

## Problem

Through `crush_lang_sdk::differential` (source → frontend → FastVM), a function with two or more parameters receives its arguments reversed. The CVM1 VMs (scheduler and `PortableVm`) bind them correctly. Pre-existing on `main` `a8247af`; found while writing CRUSH-125's differential tests. Existing differential tests didn't catch it because their multi-argument calls are symmetric (e.g. `scale(a, 3)` → `a * 3`).

## Reproduction

| source | FastVM | portable / `crush-run` |
|---|---|---|
| `fn sub(a, b) { return a - b }` `main: return sub(10, 3)` | `-7` | `7` |
| `fn three(a, b, c) { return a * 100 + b * 10 + c }` `main: return three(1, 2, 3)` | `321` | `123` |
| `fn pick(a, c, b) { if c == true { return a } return b }` `main: return pick(7, true, 9)` | `9` | `7` |

## Success criteria

- [x] all three return the CVM1 value on FastVM (and the JIT, which runs the same lowered program)
- [x] a differential test with an asymmetric multi-argument call on every backend

## Technical approach

- Compare how the callee's `store <param>` prologue pops arguments against FastVM's call convention (the CVM1 path pushes args left-to-right and the prologue stores params in reverse; check whether FastVM's lowering or its `Call` reverses again).
- `crates/crush-vm/src/fastvm/` is lane-guarded (see CRUSH-55's dispatch notes) — coordinate before landing.

## Files to modify

- `crates/crush-vm/src/fastvm/` (lowering / call)
- `crates/crush-aot/tests/differential_aot.rs`

## Resolution

Owner cleared the `fastvm/` lane guard for this work (2026-10-05). Root cause: the compiler pushes call arguments last-to-first (first argument on top for the callee's `store <param1>`), which the CVM1 VMs and AOT backends follow; FastVM's `Call` reversed them again, and the JIT's `Call` copied that. Both now leave them in place. A second, compiler-side instance: a dotted call to a program function (`m.sub(10, 3)` — walker-produced CAST) pushed first-to-last and was backwards on every VM; now last-to-first.

After: `sub(10, 3)` → 7 and `three(1, 2, 3)` → 123 on FastVM, JIT and CVM1. Tests in `crush-aot/tests/differential_aot.rs`: `asymmetric_multi_argument_calls_bind_in_order` (every backend + JIT, pinned value) and `dotted_call_to_a_program_function_binds_in_order` — both fail without the fix. No existing test encoded the reversal.
