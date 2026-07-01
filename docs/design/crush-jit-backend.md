# Crush Native Codegen Backend — Architecture & Roadmap

**Status:** Future planning document — not yet implemented.
**Last updated:** 2026-07-01

## Motivation

Add a native machine-code compilation path to crush-vm's execution stack, completing the tiers:

```
Standard CVM1 → PortableVM → FastVM (lowered) → ❌ JIT (native) ← MISSING
```

A Cranelift-based JIT removes the interpreter dispatch loop entirely, compiling each `FastInstr` sequence into straight-line native code for 10-100× speedup on hot capsule code.

## Full Document

See the plan at `/home/nixp/.cece/plans/huntress-batman-warpath.md` for the complete architecture including:

- **Key decisions**: JIT-first, Cranelift backend, hybrid nan-boxing, conservative GC for V1
- **Execution stack tiers**: Standard → PortableVM → FastVM → JIT
- **Opcode lowering**: All 84 `FastOp` → Cranelift IR mappings
- **Integration**: How ExoLight's `.cvm` dispatch wires through JIT
- **7-phase roadmap**: Skeleton → Locals/Calls → Data/Caps → Exceptions → ExoLight → Optimization → AOT
- **Risks**: Conservative GC fallback, threshold gating for latency

## Quick Reference

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Backend | Cranelift | Already in dep tree (Wasmtime), pure Rust, GC stack maps |
| Value repr | Hybrid nan-boxing | Int/float/bool in registers, heap for strings/arrays/objects |
| GC | Conservative (V1) | No stack maps needed; precise GC in V2 |
| Frames | Shadow stack (V1) | Exact parity with interpreter; native frames in V2 |
| AOT | Deferred to Phase 7 | Requires stable ABI and GC maps |
| Host calls | Trampoline escape | JIT returns `HostRequest`, trampoline dispatches, re-enters |
