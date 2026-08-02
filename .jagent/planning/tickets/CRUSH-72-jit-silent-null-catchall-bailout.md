# CRUSH-72 — crush-jit: silent TAG_NULL catch-all → Unsupported error + FastVm fallback

| Field | Value |
|-------|-------|
| **ID** | CRUSH-72 |
| **Priority** | P0 (ships wrong answers today if catch-all still present) |
| **Status** | Backlog |
| **Phase** | Correctness spine (s412) |

## Problem

`crates/crush-jit/src/compiler.rs` ends its FastOp lowering match with a
catch-all that pushes `TAG_NULL` for any opcode it doesn't implement
(July evidence: `:421`, `_ => { let cv = iconst(b, TAG_NULL); push(b, ctx, cv); }`).
Any unimplemented FastOp JIT-compiles to a silent null and execution continues —
the same program can return a different, wrong answer under the JIT than under
the interpreter, with no error raised. Contrast `fastvm/execution.rs`, which
fails loudly (`FastError::TypeMismatch`). Every production JIT (HotSpot, V8,
LuaJIT, Cranelift's own fuzzgen contract) bails to the interpreter instead.
Same bug class as the notebook NOP mapping (CRUSH-84) — systemic, not incidental.

**Re-verify at dispatch (RULES.md):** the July numbers (31 of ~86 FastOps
implemented) predate the M2 Phase 2–4 landings (ex-CRUSH-17 → CRUSH-87 work).
Count current match arms vs the `FastOp` enum in
`crates/crush-vm/src/fastvm/instructions.rs`. If the catch-all is already gone,
flip this ticket to verify+close with evidence.

## Approach

1. Catch-all → `Err(CompileError::Unsupported(op))`.
2. `JitEngine` (crush-jit/src/lib.rs) treats Unsupported as "refuse the
   function" and the caller falls back to FastVm for that function.
3. Test: iterate every `FastOp` variant; assert each either compiles or is
   refused — never fabricates a value.

## Definition of done

- [ ] No code path fabricates a value for an unimplemented opcode
- [ ] Fallback exercised by a test running a program containing an
      unsupported op through the JIT entry point → correct (interpreter) result
- [ ] Exhaustive compile-or-refuse test over all FastOp variants
- [ ] `cargo test -p crush-jit -p crush-vm` green

## Files in scope

- `crates/crush-jit/src/compiler.rs`, `crates/crush-jit/src/lib.rs`
- NOT in scope: implementing missing ops (that's CRUSH-59, M10)

## Gates

None. Gates CRUSH-59.
