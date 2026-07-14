# crush-aotc — AOT C compiler for Crush kernels

Translates a `casm::Program` (the bytecode emitted by `crush-frontend`) into a
self-contained **C translation unit** that can be compiled with any
`cc`-compatible toolchain into a `.so`, `.a`, or standalone binary.

## Architecture

```
.crush source  →  [crush-frontend]  →  casm::Program  →  [crush-aotc]  →  C source
                                                                              │  cc -O3 -march=native
                                                                              ▼
                                                                          .so / binary
```

## Pathways (from the design doc)

| Pathway | What it does | Where in code |
|---|---|---|
| **1 · Typed scalars** | Raw `int64_t`/`double` when type is statically known; no NaN-boxing | `codegen.rs` scalar fast-path |
| **2 · Type inference** | Forward-pass over CASM, proves int/float locals | `infer.rs` → `TypeMap` |
| **3 · SIMD / AVX2** | `__m256d` loops for element-wise CPU kernels | `kernel.rs` → `CpuKernelEmitter` |

## CPU kernels

A crush function compiled via `CpuKernelEmitter` is emitted as:

```c
void __crush_kernel_<name>(
    const double * __restrict__ in,
    double       * __restrict__ out,
    size_t n
);
```

With `--simd`, element-wise kernels use `_mm256_fmadd_pd` (4 doubles/iteration).

## Example

```rust
use crush_aotc::{AotcCompiler, AotcOpts};

let program = crush_frontend::compile_crush_source(source)?;
let c_src   = AotcCompiler::new(AotcOpts { opt_level: 3, ..Default::default() })
                  .compile(&program)?;
// Write c_src to a .c file and invoke cc
```

## Embedded runtime

No external runtime library is required at link time. The generated `.c` file
`#include`s an inline `crush_rt.h` with:
- `CrushValue` NaN-box typedef (same bit layout as FastVM / JIT)
- `cv_add`, `cv_mul`, etc. boxed helpers (used only on dynamic paths)
- `cap_io_print`, `cap_math_*` capability implementations
- `CV_NULL`, `CV_TRUE`, `CV_FALSE`, `cv_truthy` macros

## Relation to crush-ptx

`crush-aotc` is the **CPU counterpart** of `crush-ptx`:

| Aspect | crush-ptx | crush-aotc |
|---|---|---|
| Target | NVIDIA PTX text → ptxas | C source → cc |
| Optimizer | ptxas | cc (-O3 / LLVM / GCC) |
| Virtual registers | `%r0, %f1, …` | `_s0, _s1, …` C temporaries |
| Kernel ABI | `.visible .entry name(.param .u64 …)` | `void __crush_kernel_name(double*, double*, size_t)` |
| SIMD | PTX warp collectives / shfl | AVX2 `_mm256_fmadd_pd` |

## Example CPU kernel

See [examples/cpu/q6k_dequant_kernel.crush](../../examples/cpu/q6k_dequant_kernel.crush)
for the Q6_K dequant scalar + AVX2 path.
