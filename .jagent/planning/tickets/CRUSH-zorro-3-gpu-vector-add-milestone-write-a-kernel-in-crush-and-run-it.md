# CRUSH-zorro-3 — GPU vector-add milestone: write a kernel in .crush and run it on the GPU (gpu.* intrinsics + gpu.launch)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-3 |
| **Priority** | P2 |
| **Status** | Done — merged #186 (0360dc3), 2026-10-15 |
| **Phase** | M11 |
| **Assignee** | vega |
| **Dependencies** | crush-ptx (CRUSHAST-PTX-Q6K); relates to CRUSH-210 (no CLI path to PTX) |
| **Estimated effort** | M |
| **Branch** | `agent/vega/crush-gpu-vecadd` |

## Problem

crush-ptx could compile a CASM program to PTX, but only one assembled by hand in Rust. Crush source had no way to express a kernel (thread ids, global loads and stores), and nothing loaded or launched the PTX, so no `.crush` program could run on a GPU. `examples/gpu/q6k_gemv.crush` was illustrative only ("NOT yet compilable").

## Success criteria

- [x] A `.crush` file defines a kernel with device intrinsics, and the same file's host code launches it: `crush-run run --gpu examples/gpu/vec_add.crush` prints `3.0`, `2997.0`, `0` (0 mismatches over 1000 elements vs the CPU) on the 5070 Ti.
- [x] The kernel compiles to one standalone PTX entry that `ptxas` assembles (crush-ptx `test_vec_add_crush_source_to_ptx`).
- [x] The host program still compiles to CVM1 when a kernel is in the file, and running without `--gpu` is refused by the preflight, naming `--gpu: gpu.launch`.
- [x] Off by default: without the `gpu` feature, nothing changes and `--gpu` is a clear error.

## Technical approach

- **Frontend** (`crush-frontend/src/compiler.rs`): dotted calls named in `GPU_INTRINSICS` lower to `ptx_*` CASM ops:
  - `gpu.thread_idx()`, `gpu.block_idx()`, `gpu.block_dim()`, `gpu.global_id()`
  - `gpu.load_f32(buf, i)`, `gpu.store_f32(buf, i, v)` (they take an element index and scale it by 4)
  - `gpu.f32(x)`

  Integer results are widened to s64, so they type-check against the kernel's s64 params. A user variable or function named `gpu` still wins. `gpu.launch` stays a normal capability call.
- **crush-ptx:**
  - `push_null` and `pop` are accepted, so expression statements and an implicit end-of-function return work in kernels.
  - A same-type `cvt` is a no-op.
  - New `compile_kernel(program, name)` compiles one function with a fresh compiler (unlike `compile_program`, which shares registers and locals across functions).
  - New `is_device_function`.
- **CVM1 lowering** (`crush-lang-sdk/src/compile.rs`): functions that use `ptx_*` ops are skipped; they only run on the device.
- **`gpu.launch`** (`crush-lang-sdk/src/gpu.rs`, feature `gpu` = crush-ptx + cudarc 0.11):
  - Compiles the named kernel from the program's CASM on first use and loads it through the CUDA driver.
  - Uploads array arguments as f32 buffers and passes integers as u64.
  - Launches `ceil(threads/256)` blocks of 256, then copies every buffer back into its array in place.
  - Granted by `crush-run run --gpu`, effect `gpu/launch`.

## Non-goals (follow-ups)

- f32 scalars, non-f32 buffers (u8/f16/i32), 2-D/3-D grids, and a user-chosen block size.
- Device-resident buffers across launches (every launch re-uploads).
- Narrowing crush-ptx's integer math to 32-bit. CASM's i64 integer model was measured to cost about 1.8× against an LLVM-compiled kernel running the same algorithm.
- Making `examples/gpu/q6k_gemv.crush` compile (it needs u8/f16 loads and warp shuffles as intrinsics).
- `.casm`/`.cvm1` inputs with `--gpu` (only `.crush` hands the capability its CASM).

## Release note

**crush-ptx (0.1.0) is not on crates.io.** crush-lang-sdk's new optional dependency on it will block `cargo publish` of crush-lang-sdk until crush-ptx is published (or the `gpu` capability moves to its own crate).
