# CRUSH-zorro-50 — crush-ptx: compile_program emits one .version/.target header per kernel (invalid PTX for 2+ kernels)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-50 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M5 (backends) |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude (vega) — 2026-10-15, GPU viability test on the RTX 5070 Ti |

## Problem

`crush_ptx::compiler::compile_program` compiles every function of the program and concatenates the
results, but `PtxCompiler::compile_function` writes the module header (`.version 7.5`, `.target
sm_80`, `.address_size 64`, `crates/crush-ptx/src/compiler.rs:79-80`) into **each** function's
text. A program with two or more kernels therefore produces invalid PTX:

```text
ptxas output.ptx, line 60; fatal : Parsing error near '.version 7.5': syntax error
```

Found running two hand-built elementwise kernels (`map_affine`, `map_poly`) through
`compile_program` during the GPU viability test (2026-10-15). `compile_kernel` (one function per
module, used by `gpu.launch`) is unaffected; `compile_program` also shares one `PtxCompiler`
(registers, locals) across functions, which `compile_kernel`'s doc comment already warns about.

## Fix

Emit the header once per module, then each `.entry`/`.func` body, each compiled with a fresh
compiler state (or reset registers/locals between functions). Or deprecate `compile_program` in
favour of `compile_kernel` if a multi-entry module has no user.

## Done when

- [ ] A two-kernel program compiles to one module that `ptxas -arch=sm_80` accepts and both entries
      load from (test with `ptxas` when available, like the existing integration tests).
- [ ] crush-ptx's README no longer says the frontend has no `ptx_*` intrinsics (stale since
      CRUSH-zorro-3 / #186).
