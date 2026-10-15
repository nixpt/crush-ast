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

## Semantics to pin (before implementing)

- **Code points vs bytes**: `"hello"[1:3]` must index by Unicode scalar (code point), not UTF-8 byte — `"héllo"[1:3]` → `"él"`, not a byte-split garble. Reuse the VM's existing Unicode-scalar handling (`conv.chr`/`conv.ord`, CRUSH-117).
- **Negative ends**: decide and document `"hello"[-3:]`, `"hello"[:-1]`, `"hello"[-1]` — either mirror array-slice semantics or refuse loudly.
- **Out-of-range ends**: clamp like array slices (`xs[1:99]` → whole tail), never panic.

## Success criteria

- [ ] `"hello"[1:3]` → `"el"` on CVM1 and PortableVM.
- [ ] FastVM/JIT/AOT either match the VM result or **refuse loudly** (no silent NOP / wrong substring) — mirror the CRUSH-13 "exact-or-loud" rule.
- [ ] A parity test covers the string-slice result across backends.

## Technical approach

- Extend the slice opcode path to accept `Value::Str` and return a substring, indexing by code point and clamping ends like array slices.

## Files to modify

- `crates/crush-vm/src/portable_vm.rs` / `scheduler.rs` — slice opcode: accept string, code-point index, clamp ends
- `crates/crush-vm/src/fastvm/` — implement-or-refuse-loudly
- `crates/crush-aot/src/codegen*.rs` — AOT parity or loud refusal

## Non-goals

- Step slicing `xs[::2]`
