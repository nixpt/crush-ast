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

## How to test on a GPU box
Because `ptxas` is unavailable via `buckets` on the `[main]` box, `ptxas --verify` could not be wired directly into `cargo test`. You must verify the emitted PTX yourself.

1. **Emit PTX**:
   You can capture the emitted PTX by running the integration tests with stdout captured:
   ```bash
   cargo test -p crush-ptx -- --nocapture > output.ptx
   ```
   (Alternatively, wire `crush-ptx::compile_program` into your local `crush-aot` / `crush run` pipeline to emit PTX from your `.crush` files).

2. **Verify with ptxas**:
   ```bash
   ptxas --gpu-name sm_80 output.ptx -o output.cubin
   ```
   *If `ptxas` throws register or alignment errors, the emitter needs tweaks.*

3. **Launch**:
   Once you have the `.cubin` or raw PTX, load it via the CUDA driver API (`cuModuleLoadData`) using your zorro runtime or `ironsand`.

## What pyptx taught us
- **ptxas is the optimizer**: We do zero register allocation. We simply hand out fresh `%r`, `%f`, `%p` virtual names and let `ptxas` handle the graph coloring and scheduling offline.
- **Predicate tracking**: Branching (`if_`, `loop`) uses simple predicate `.pred` registers (`%p`) that we assign from compare operations.
- The entry signature needs strict `.param` alignment mappings for u64 buffers.

## Note on verification
`ptxas --verify` did not run on `[main]`. The constraint is captured as a gap in `TASKS.md`. Please do not blindly trust the generated PTX until you run `ptxas` on it.
