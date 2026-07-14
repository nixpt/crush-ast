# crush → PTX backend — vocabulary & design (draft, Vega s371)

Sibling to `crush-jit-backend.md` (the CASM→Cranelift **CPU** JIT). This doc
pins the vocabulary for a CASM→**PTX** GPU backend so crush kernels can run on
NVIDIA GPUs — the "kernels in crush" thread.

Status: **design / discussion.** No code yet. This is the turn-3 artifact of a
multi-turn design arc (borrowing from pyptx, the razor megakernel, haiku_san).

## Context — what we're borrowing from

Reference projects surveyed in `/workspace/scratch`:

- **pyptx** (`scratch/pyptx`, Apache-2.0, ~11.7k LOC) — Python DSL → handwritten
  PTX. Real parser + emitter + transpiler, round-trips 218 PTX files
  byte-identical. Validated on sm_120 + RTX Pro 6000 Blackwell (our two cards),
  64–78% of cuBLAS GEMM, ~10× torch on grouped-GEMM MoE. **The design blueprint
  (and the license lets us lift code).**
- **nvvm-probe** (`scratch/nvvm-probe`) — drives libnvvm (LLVM-IR→PTX) via
  `ironsand-gemv/crates/nvvm`+llvm19. Backs the LLVM path (Way 1).
- **ironsand** (`projects/ironsand`) — Rust→PTX via LLVM19. Backs Way 1.
- **burn/cubecl** (`scratch/refernces_for_zorro/burn`) — multi-backend GPU
  compiler (cuda/hip/wgpu/metal/vulkan/webgpu). The vendor-neutral path.

## The load-bearing insight: **ptxas is the optimizer**

pyptx does **no register allocation and no optimization**. It hands out fresh
virtual names (`%r0, %r1, …`), declares them, emits textual PTX, and lets
**ptxas** (at `cuModuleLoadData` JIT time, or offline) do physical RA,
scheduling, and optimization. `reg.py`'s 1295 lines are naming/declaration
bookkeeping, not graph-coloring.

Consequence for Way 1 (LLVM/nvvm) vs Way 3 (crush-owns-PTX):

- **Way 1**: crush → LLVM-IR → libnvvm → PTX → ptxas. *Two* optimizers, full
  LLVM weight, crush emits SSA it doesn't control.
- **Way 3**: crush → PTX text → ptxas. *One* optimizer. A stack machine (CASM)
  lowers to "fresh named reg per push" trivially — we never reuse/color
  registers, so the main reason to want LLVM evaporates.

**Way 3 is recommended on merit** (structurally simpler; ptxas is the shared
downstream backend either way), not just ethos. Way 1 stays the fallback if the
spike stalls.

## The model to adopt

A **tracing builder**, not a compiler IR. A `TraceContext` accumulates
`Instruction`/`RegDecl`/`Label`; each op appends; serialize to PTX text at the
end. CASM is already a linear instruction stream — the port is CASM → PTX text
(+ a decl prologue), with structured `if/loop` → labels + predicated `bra`
(pyptx `if_`/`loop`/`range_` show the exact lowering).

## The pinned vocabulary (tiered)

| Tier | Vocabulary | CASM today | Gap |
|---|---|---|---|
| **0 · SIMT model** | `thread_idx.{x,y,z}`, `block_idx`, `block_dim`, `grid_dim`, `lane_id`, `warp_id`; `@kernel` entry + param space; `barrier_sync()` | none | new intrinsics → `mov.u32 %r,%tid.x`, `.visible .entry`, `bar.sync`. pyptx `_Special`/`global_ptrs`. |
| **1 · Memory** | address spaces (global/shared/local/param); typed `ld`/`st`; `.shared` arrays w/ align | stack is space-less | new `LOAD_<space>_<ty>`/`STORE_…` + `var()` decl. pyptx `var`/`_make_address`. |
| **2 · Typed scalar ALU** | add/sub/mul/mad/shl/shr/and/or/xor/min/max **with dtype**; `setp`→predicate; `cvt` (u8→f32, f32→f16, rounding); `fma.rn` | has ops, **untyped** | attach dtype (pyptx infers modifier from `Reg.dtype`); add `CVT` + predicate-producing compare. `cvt` is the quant-dequant workhorse. |
| **3 · Control flow** | structured `if/else/for/loop` → predicated `bra` + labels | has `jmp`/cond-jump | add **predicated** branch (`@%p bra`); structured→label lowering. pyptx `if_`/`range_`. |
| **4 · Warp collectives** | `shfl.sync.{bfly,idx,up,down}`; `warp_reduce_sum/max/min` | none | one primitive `SHFL`; `warp_reduce` is then crush **library code** (pyptx builds it from shfl). Needed for GEMV reduction. |
| **5 · Tensor-core / async** | wgmma / mma.sync / tcgen05, cp.async, TMA, mbarrier | none | **DEFER.** Not needed for GEMV; pyptx has full reference impls to borrow at GEMM/flash-attn. |

## Spike: Q6_K dequant → GEMV (Tiers 0–4, ~20 opcodes)

Chosen because it exercises the load-bearing subset (quant-decode + warp
reduction), is the actual decode hot-path, and has a **byte-exact oracle** (our
existing Q6_K vecdot).

- T0: `thread_idx.x`, `block_idx.x`, `block_dim.x`, `lane_id` + `@kernel`
- T1: `ld.global.u8` (quant bytes), `ld.global.f16` (scales/act), `st.global.f32`
- T2: `shr`/`and` (unpack 6-bit), `cvt.f32.s8`, `fma.rn.f32` (Q6_K `d·q` accum)
- T3: one `for k in range` over the row
- T4: `warp_reduce_sum` (shfl.bfly) → dot product

No tensor cores, no async copy, no mbarrier.

## Where the megakernel + haiku_san fit

Not competing codegen paths — the **layer above** the emitter:

- **razor megakernel / opgraph** → a crush program that inlines several ops
  before emission (fusion = crush-level inlining → one `crush→PTX`).
- **haiku_san DAG** → crush emits N kernels + a launch schedule.

The emitter is the foundation both were missing.

## "GPU without CUDA" — two meanings

1. **No CUDA toolchain (nvcc/runtime/CUTLASS), NVIDIA driver present** — PTX via
   the CUDA **driver API** (`cuModuleLoadData`). Already how cudarc/ironsand run.
   **Free with Way 1 or Way 3.**
2. **No NVIDIA at all (AMD/Apple/Intel)** — needs SPIR-V/Vulkan or WGSL. cubecl
   gives it turnkey; a crush-owned SPIR-V target (a second emitter backend, like
   cubecl has many) gives it under our control later. pyptx is NVIDIA-only.

## Open decisions (for the next turns)

- Way 3 (crush owns PTX, port pyptx design) vs Way 1 (crush→LLVM→nvvm). Leaning
  Way 3 on merit.
- Spike target sm_120-only, or SPIR-V/vendor-neutral in scope from the start
  (pushes toward a cubecl-style multi-backend IR vs a pyptx-style direct emitter).
- Crush *surface syntax* for the SIMT intrinsics (`@thread_idx`, `@shared`,
  `@sync`, warp ops) — the actual language extension.
