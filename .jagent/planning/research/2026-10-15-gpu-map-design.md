# `gpu_map` — data-parallel maps on the GPU (design, draft 1)

Status: **draft for review** (vega → foreman → owner), 2026-10-15. Ticket: CRUSH-zorro-51.
Builds on CRUSH-zorro-3 (`gpu.launch`, #186) and `parallel_map` (#175 / #207).

## Why

On the RTX 5070 Ti in the dev box (measurements 2026-10-15, scripts in
`/workspace/scratch/gpu-viab/` on that box), for an elementwise map that does real work per element
(64 dependent multiply-adds), 16M f32 elements:

| path | time |
|---|---|
| crush VM, one thread (release) | ≈350 s (340 ns per iteration) |
| crush `parallel_map`, 32 cores | ≈26 s |
| Rust, one thread / 32 threads (SIMD) | 326 ms / 21 ms |
| crush-ptx kernel via CUDA, **including** host↔device copies | **12 ms** (kernel alone 0.3 ms) |
| same kernel as WGSL via wgpu/Vulkan on the same GPU | kernel 0.28 ms |

and crush-ptx's Q6_K GEMV kernel — the first time crush-generated PTX ran on real hardware —
was correct for every row on four shapes up to 11008×4096 (error 2–3e-8 of Σ|terms|, 10× tighter
than a sequential f32 CPU loop), 0.4 ms vs 129 ms single-threaded C.

Two lessons shape the design:

1. **The GPU wins only when there is enough work per byte moved, or the data stays on the
   device.** A memory-bound map (`y = 2x + 1`) is *slower* end to end than one CPU thread,
   because the copies dominate (16M: 29 ms vs 11 ms). One call must not pretend otherwise.
2. **Most of the gap to crush today is the interpreter**, not the GPU: the same map is ~1000×
   faster on the GPU than through `parallel_map`, but a well-written CPU loop is within ~2× of
   the GPU end to end. `gpu_map` is for kernels where that ~2× (and the 70× kernel-only margin
   when data stays resident) matters — numeric batches, image/signal processing, quantised
   inference — not a general speed-up for crush code.

## What exists (CRUSH-zorro-3)

- Device intrinsics in crush source: `gpu.global_id()`, `gpu.thread_idx()`, `gpu.block_idx()`,
  `gpu.block_dim()`, `gpu.load_f32(buf, i)`, `gpu.store_f32(buf, i, v)`, `gpu.f32(x)`. A function
  that uses them is a kernel: crush-ptx compiles it (`compile_kernel`) and CVM1 lowering skips it.
- `gpu.launch("kernel", threads, args…)` (`crush-lang-sdk/src/gpu.rs`, feature `gpu`, cudarc
  0.11): uploads array args as f32 buffers, passes integers as u64, launches ⌈threads/256⌉×256,
  copies every buffer back in place. Granted by `crush-run run --gpu`.
- Verified on this box: `examples/gpu/vec_add.crush` → 0 mismatches; the 64-FMA map written
  as a crush kernel runs 1M elements in 1.0 s total, 0.41 s of it building the input arrays in
  the VM.

## The proposal

```crush
let ys = gpu_map(xs, |x| {
    let v = x
    let k = 0
    while k < 64 {
        v = v * v * 0.5 + 0.25
        k = k + 1
    }
    return v
})
```

`gpu_map(xs, f)` = `parallel_map(xs, f)` — same contract: results in input order, `f` sees each
element, captured values are read-only (a write is an error) — but `f` is compiled to a GPU kernel.
The user writes no indices, no launch, no buffers.

### 1. Kernel subset (checked at compile time)

The closure body (and functions it calls, inlined) may use: numbers, `+ - * /` and `%` on integers,
comparisons, `and`/`or`/`not`, `if`/`else`, `while` and counted `for`, local `let`s and
assignment, `return`, and a short list of math builtins with exact or correctly-rounded GPU
equivalents (`sqrt`, `abs`, `floor`, `ceil`, `min`, `max`, `fma`). Captured values may be numbers
(become kernel parameters) or numeric arrays (become read-only device buffers, indexed by `xs[j]`).

Anything else — strings, maps, allocation (`[]`, `push`), capabilities (`print`), calling a function
value, recursion, writing a captured value — is a **compile error naming the construct**
("`print` can't run inside a gpu_map kernel"), never a silent fallback. (Exact-or-loud, as everywhere.)

### 2. Numbers: f32 — the owner's call

crush floats are f64. Consumer GPUs run f64 at 1/64 of f32 throughput (the 5070 Ti included), and
WGSL has no f64 at all. Options:

- **(A, recommended) `gpu_map` computes in f32**, explicitly: the input array is converted to f32,
  every operation in the kernel rounds to f32, the result is an f32 array. The CPU fallback (§4)
  performs the **same f32 operations**, so GPU and CPU results are bit-identical; results differ
  from the f64 `parallel_map` by f32 rounding (measured: `0.29289323` vs `0.2928932188` for the
  map above). Integers are i32 in kernels (i64 math cost ≈1.8× in crush-ptx, ZORRO-CRUSH-PTX-2).
- (B) f64 kernels on CUDA only: exact vs `parallel_map`, ~30–60× slower kernels, no WGSL backend.
- (C) Per-call choice (`gpu_map(xs, f, f64: true)`), defaulting to A.

Bit-identity of A requires: no contraction of `a*b+c` into FMA unless written as `fma(a,b,c)`
(crush-ptx emits `mul.rn`, which ptxas won't fuse — verified: no FFMA in the SASS), division and
`sqrt` correctly rounded (`div.rn`, `sqrt.rn`), and IEEE semantics for f32 division by zero
(`inf`/`nan`, not crush's DivByZero error — documented). Transcendentals (`sin`, `exp`, …) are
not bit-identical across GPU/CPU and are excluded from v1.

### 3. Data: packed f32 arrays

Converting a crush array of `Value::Float` to an f32 buffer and back costs a pass over the data in
the VM (part of the 0.6 s above). Add a packed numeric array value — `f32s([...])` /
`f32s(n, fill)` → `Value::F32Array(Rc<RefCell<Vec<f32>>>)`, indexable, `len`, printable, convertible
back with `list(...)`. `gpu_map` accepts a plain numeric array (converted, documented cost) or an
`F32Array` (zero conversion; uploaded as-is) and returns an `F32Array`. The same type is what a
later device-resident API hands around.

### 4. Execution and fallback

- The compiler lowers the closure to a **kernel function** (the CRUSH-zorro-3 form: a `ptx_*`
  CASM function with `i = global_id(); if i < n { y[i] = body(x[i]) }`), alongside the ordinary
  lifted lambda, and the `Func` value carries the kernel's name.
- `gpu_map` is a capability. With a GPU and the `gpu` grant: compile the kernel (cached per
  program, like `gpu.launch`), upload `xs` and the captured arrays, launch, download `ys`.
- **Without a GPU** (no device, no `gpu` feature, not granted, wasm without WebGPU, FastVM/AOT):
  run the same kernel semantics on the CPU — the `parallel_map` workers executing the lifted
  lambda with f32 rounding at each operation (a small `f32` mode in the arithmetic caps, or a
  dedicated kernel interpreter). Same results, slower, no warning needed — like `parallel_map`'s
  sequential fallback.
- `CRUSH_GPU=off|cuda|wgpu|auto` selects; `auto` prefers a discrete GPU.

### 5. Backends: PTX now, WGSL next

- **PTX / CUDA** (exists: crush-ptx + cudarc). NVIDIA only. Fix CRUSH-zorro-50 first.
- **WGSL / wgpu** (new emitter, `crush-wgsl`, from the same kernel CASM): Vulkan, Metal, DX12, GL
  and **WebGPU — crush-web in the browser**. Measured at the same kernel speed as PTX on the same
  card. surfer-browser already ships wgpu 27 (via vello); its `wgpu_context` 0.3 has device setup
  (`WGPU_BACKEND` / `WGPU_ADAPTER_NAME` env) and buffer readback to reuse. Note: on the dev box
  `VK_LOADER_DRIVERS_SELECT=*radeon*` hides the RTX from Vulkan (CUDA unaffected) — adapter choice
  must be explicit and reported.

### 6. Errors and safety

- Out-of-range `xs[j]` on a captured array: kernels can't throw — each thread writes an error code
  to a per-launch flag buffer; after the launch the host raises the first failing **item in input
  order** ("gpu_map: item 17: index 99 out of range (len 10)"), matching `parallel_map`.
- Integer division by zero: the same flag. f32 division by zero: IEEE (§2).
- A runaway kernel (an endless `while`): a per-launch watchdog / iteration budget in the kernel
  (`steps` counter checked in loops), surfaced as the instruction-quota error.
- Permission: GPU access is a privileged capability (`gpu`, granted like `--fs`); without the grant
  `gpu_map` runs on the CPU fallback (results identical), and `--gpu-required` makes that an error
  for users who must not silently run slowly.

### 7. Not in v1

Device-resident buffers across calls (`gpu.upload(xs)` → handle), reductions (`gpu_reduce`),
shared memory and warp collectives, 2-D grids, f16/u8 element types, multi-GPU. Each is its own
ticket after v1; residency is the first, since it is what makes memory-bound and inference
workloads pay off (Q6_K weights uploaded once).

## Phases

1. **crush-ptx fixes** — CRUSH-zorro-50 (one header per module); README update.
2. **Packed f32 arrays + the kernel subset checker + CPU fallback** — no GPU needed, fully
   testable in CI; `gpu_map` already works (on the CPU) and its results are pinned.
3. **PTX path** — `gpu_map` lowering to a zorro-3 kernel, launch through the existing runtime,
   parity tests GPU vs fallback (bit-identical), skipped where no GPU.
4. **WGSL / wgpu** — `crush-wgsl` emitter + wgpu runtime; crush-web WebGPU after.
5. Residency, reductions (separate tickets).

## Open questions for the owner

1. **Precision:** f32 (A), f64 CUDA-only (B), or per call (C)?
2. **Name / surface:** `gpu_map(xs, f)` (proposed), or a kernel annotation (`@gpu fn`) feeding
   `parallel_map`, or `parallel_map(xs, f, gpu: true)`?
3. **Permission default:** privileged (`--gpu` grant, CPU fallback without) as proposed?
4. **Backend order:** finish PTX (NVIDIA) first as proposed, or go straight to wgpu for
   portability (and crush-web)?
5. **Packed arrays:** `F32Array` only, or a general typed-array family (`i32s`, `f64s`) now?
