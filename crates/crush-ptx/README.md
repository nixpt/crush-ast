# CRUSHAST-PTX-1: crush-ptx backend

This crate implements the CASM -> PTX emission backend for crush kernel programs (Way 3 of the design doc).

## What works
- **Tier 1 (Memory)**: `.param` parameters, `ld.global` and `st.global` (f32/f64/u32).
- **Tier 0 (SIMT model)**: `thread_idx.x` and `block_idx.x` (`%tid.x`, `%ctaid.x`).
- **Tier 2 (Basic scalar ALU)**: `add`, `sub`, `mul`, `div` across basic datatypes (f32/f64/u64/s64) using virtual registers.
- **Tier 3 (Control Flow)**: `jmp`, `jmp_if`, `jmp_if_not`.
- Unimplemented opcodes correctly **HARD ERROR**. There is no silent `TAG_NULL` continuation like in the JIT. A test proves this.

## What doesn't
- **Structured Control Flow / Loops**: Simple predicate jumps are emitted, but a full lowering pass that emits clean PTX loop constructs isn't built yet.
- **Tier 4 (Warp Collectives)**: `shfl`, `warp_reduce`.
- **Tier 5**: Tensor Cores / async copy.
- **Memory spaces**: Only global and param memory are wired up; shared memory sizing and `.shared` declarations are missing.
- End-to-end `@gpu` capabilities aren't handled beyond stubbing the boundary, as they are runtime-side.

## `ptxas` verification (update: it works via `buckets`)

`ptxas` **is** reachable on the `[main]` (no-GPU) box after all — not through `buckets`' pkgx-backed
pantry (no `nvcc`/`cuda` package exists there), but as a **pip wheel**: NVIDIA publishes `ptxas`
itself as `nvidia-cuda-nvcc-cu12` on PyPI, installable into an ephemeral `buckets` python sandbox.
`ptxas` is a pure text→cubin assembler — it needs a target arch (`-arch=sm_XX`), not physical GPU
hardware, so this genuinely verifies the emitted PTX without a GPU box:

```bash
# network is required for the pip fetch itself, so this one step needs --no-sandbox;
# ptxas usage afterward doesn't
buckets run --no-sandbox python@3.11 -- pip install --break-system-packages nvidia-cuda-nvcc-cu12
PTXAS=~/.local/lib/python3.14/site-packages/nvidia/cuda_nvcc/bin/ptxas

# emit PTX — wire crush-ptx::compiler::compile_program(&program) into a scratch bin,
# or capture it from a test
"$PTXAS" -arch=sm_80 output.ptx -o output.cubin   # exit 0 + a real cubin = valid PTX
```

**This actually ran and found two real bugs**, both now fixed (see `src/compiler.rs`, both
covered by `tests/integration.rs`):
- `cvt` float→int emitted no rounding modifier (`cvt.s32.f64` instead of `cvt.rni.s32.f64`) —
  ptxas: *"Rounding modifier required for instruction 'cvt'"*.
- `div` on floats had the same gap (`div.f64` instead of `div.rn.f64`) — same ptxas error, same
  fix. Integer div/all other ops in that bundle (add/sub/mul/bitwise/shift) don't need one.

Neither was ever run through `ptxas` before (this crate's own tests only asserted on the Rust
`String` output, never fed it to the real assembler) — exactly the failure mode the design doc
warned about. Tier 0–3 (SIMT model, memory, scalar ALU incl. fma/cvt/div, control flow) now
compiles to a real cubin end-to-end on a CUDA 12.9 toolchain. **Still unverified**: whether the
cubin actually *runs correctly* on real hardware (launch/execute semantics), Tier 4/5
(warp collectives, tensor cores), and structured loop lowering — `ptxas` only proves the PTX is
well-formed, not that the kernel is correct. That part still needs a real GPU box (zorro/A16/RunPod).

Once you have a `.cubin`, load it via the CUDA driver API (`cuModuleLoadData`) using your zorro
runtime or `ironsand`.

## What pyptx taught us
- **ptxas is the optimizer**: We do zero register allocation. We simply hand out fresh `%r`, `%f`, `%p` virtual names and let `ptxas` handle the graph coloring and scheduling offline.
- **Predicate tracking**: Branching (`if_`, `loop`) uses simple predicate `.pred` registers (`%p`) that we assign from compare operations.
- The entry signature needs strict `.param` alignment mappings for u64 buffers.

## Note on verification
`ptxas` now runs on `[main]` via the pip-wheel path above and has been used to verify Tier 0–3
output (see the section above for what it caught). This closes the gap `TASKS.md` previously
flagged — but it's static-assembly verification only, not a substitute for launching the kernel on
a real GPU.
