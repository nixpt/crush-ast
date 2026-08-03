# CRUSH-61 — AOT-from-JIT: dump compiled native code as .so (cold-start-free deploys)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-61 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M10 |

## Problem

JIT compilation cost is paid per process start. ROADMAP M10 wants the JIT's
compiled output dumpable as a `.so` for cold-start-free deployment.
⚠ NAME COLLISION (s412 triage): git log's "M2 Phase 7" commits (9c4d2d5,
52c1e07) are JIT-into-differential-pipeline wiring — a DIFFERENT "Phase 7".
Do not grep those commits expecting AOT-dump work; none exists.

## Approach

Cranelift object-module path: compile FastOps via cranelift-object into a
relocatable object + link to .so; runtime stubs (host caps, value model)
resolved at load. Loader path in crush-lang-sdk (`crush-run --prejit x.so`).
Differential fixture: dumped .so result ≡ live-JIT ≡ interpreter. Note the
seam vs crush-aot's C backend honestly (two native paths; this one shares the
JIT's semantics by construction — that's its argument).

## Definition of done

- [ ] Dump + load + run works for corpus programs; differential-verified
- [ ] Cold-start bench: .so load vs JIT-compile quoted
- [ ] Relationship to crush-aot documented (when to use which)

## Files in scope

- `crates/crush-jit` (object emission), `crates/crush-lang-sdk` (loader)

## Gates

CRUSH-60.
