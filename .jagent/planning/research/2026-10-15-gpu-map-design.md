# `gpu_map` — data-parallel maps on the GPU (design, draft 2)

Status: **draft 2** — draft 1 critiqued by the foreman (per-backend identity, integer overflow, the
kernel-interpreter fallback, one shared runtime), folded in below; for the owner. 2026-10-15.
Ticket: CRUSH-zorro-51. Related: CRUSH-zorro-52 (auto-offload of built-ins).
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
than a sequential f32 CPU loop), 0.4 ms vs 129 ms single-threaded C. (That is evidence the PTX
*runs correctly*, not of bit-identity: it is a reduction, summed in a different order from the CPU
loop. v1 has no reductions.)

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
  every operation in the kernel rounds to f32, the result is an f32 array. Results differ from the
  f64 `parallel_map` by f32 rounding (measured: `0.29289323` vs `0.2928932188` for the map above) —
  which is why the name is explicit (`gpu_map`, not a flag on `parallel_map` that would hide the
  precision change).
- (B) f64 kernels on CUDA only: exact vs `parallel_map`, ~30–60× slower kernels, no WGSL backend.
- (C) Per-call choice (`gpu_map(xs, f, f64: true)`), defaulting to A.

**What "same results" means, per backend** (the reference is the kernel interpreter, §4):

| | floats | integers |
|---|---|---|
| **PTX** | **bit-identical**: `+ - *` and `div.rn`, `sqrt.rn` correctly rounded; no contraction (`mul.rn` isn't fused — verified, no FFMA in the SASS); **no `.ftz`** (subnormals kept; a subnormal-input test pins it) | identical |
| **WGSL / Vulkan** | **within N ULP** (N measured and pinned): WGSL's `/` is 2.5 ULP and `sqrt` is not correctly rounded; SPIR-V may contract `a*b+c` unless decorated NoContraction; Vulkan may flush subnormals | identical |

Details both paths must agree on:
- **Integers are i32 in kernels** (i64 math costs ≈1.8× in crush-ptx). crush ints are i64 and
  overflow loudly, so a kernel **checks i32 overflow** on `+ - *` and reports it through the error
  flag buffer (§6) — loud, like crush, never a silent wrap. Captured ints are range-checked to i32
  at launch; float→int conversion follows crush's rule (truncation toward zero, out-of-range is an
  error), not PTX's saturating `cvt.rzi`.
- **NaN in `min`/`max`:** PTX and Rust return the non-NaN operand; WGSL leaves it unspecified —
  the WGSL backend emits an explicit select, or NaN inputs are outside the WGSL ULP claim.
- **Constants:** crush literals are f64; each is rounded to f32 **once**, when the kernel is
  compiled, and both paths use that f32 value.
- **Division by zero:** f32 IEEE (`inf`/`nan`), documented — not crush's DivByZero error. Integer
  division by zero: the error flag.
- **Printing:** an `F32Array` element prints the shortest digits that round-trip **as f32**
  (`0.29289323`), not its f64 widening (`0.2928932310…`).
- Transcendentals (`sin`, `exp`, …) aren't correctly rounded on any GPU: excluded from v1.

### 3. Data: packed f32 arrays

Converting a crush array of `Value::Float` to an f32 buffer and back costs a pass over the data in
the VM (part of the 0.6 s above). Add a packed numeric array value — `f32s([...])` /
`f32s(n, fill)` → `Value::F32Array(Rc<RefCell<Vec<f32>>>)`, indexable, `len`, printable, convertible
back with `list(...)`. `gpu_map` accepts a plain numeric array (converted, documented cost) or an
`F32Array` (zero conversion; uploaded as-is) and returns an `F32Array`. The same type is what a
later device-resident API hands around. Design it so JS `Float32Array` (and later Python
`array('f')`) share the representation — a COSM-adjacent value type (#206); raise it there when
COSM K2 does protocols. Other typed arrays (i32, f64) later.

### 4. Execution and fallback

- The compiler lowers the closure to a **kernel function** — the CRUSH-zorro-3 form, a `ptx_*` CASM
  function `i = global_id(); if i < n { y[i] = body(x[i]) }` — and the `Func` value carries the
  kernel's name. (The ordinary lifted lambda is not used by `gpu_map`.)
- **`gpu_map` is a VM built-in that always works.** Its CPU path is an **interpreter over the
  kernel CASM** — the *same* kernel function the GPU runs, with its types (i32, f32, `cvt`) — not
  the lifted lambda run with f32 arithmetic, which would be a second lowering of the same source
  and drift (integer typing, constants, crush-ptx's type inference). The `ptx_*` subset is small
  and typed: a few hundred lines, deterministic, run in chunks over `parallel_map`'s workers. It
  doubles as a GPU-free oracle in CI for every kernel crush-ptx emits.
- **A GPU executor is optional and host-registered** (crush-lang-sdk feature `gpu`): when present,
  granted and not disabled, the built-in hands the kernel to it. crush-vm stays free of cudarc.
- Fallback cases — no executor (no device / feature / grant), wasm without WebGPU, FastVM/AOT,
  `CRUSH_GPU=off` — run the interpreter, with identical results (PTX) and no error. The slowdown can
  be ~1000×, so: `gpu.backend()` reports what ran (`"cuda"`, `"wgpu"`, `"cpu"`), verbose mode prints
  a notice, and `--gpu-required` turns the fallback into an error.
- `CRUSH_GPU=off|cuda|wgpu|auto` selects; `auto` prefers a discrete GPU.

### 5. Backends: PTX now, WGSL next

**One shared GPU runtime**, refactored out of CRUSH-zorro-3's `crush-lang-sdk/src/gpu.rs`: a
backend trait (CUDA now, wgpu later), device/context selection, a compiled-module cache, typed
buffers with upload/download, and launch. `gpu.launch` (explicit kernels) and `gpu_map` are two
front ends on it — one permission story, one place to test.

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
- A runaway kernel (an endless `while`): an iteration budget — one counter per loop-back edge,
  the budget passed as a **launch parameter** (so tests can trigger it cheaply) — surfaced as the
  instruction-quota error through the flag buffer.
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
2. **Packed f32 arrays + the kernel subset checker + the kernel-CASM interpreter** — no GPU
   needed, fully testable in CI; `gpu_map` already works (on the CPU) and its results are pinned.
3. **Shared runtime + PTX path** — refactor `gpu.rs` into the runtime (§5), `gpu_map` kernels
   launched through it. Parity bar: **bit-identical** to the interpreter (floats incl. subnormals,
   integers incl. overflow errors), skipped where no GPU.
4. **WGSL / wgpu** — `crush-wgsl` emitter + wgpu backend. Parity bar: **≤ N ULP** for floats
   (N measured), identical for integers. crush-web WebGPU after.
5. Residency, reductions (separate tickets).

## Open questions for the owner

1. **Precision:** f32 (A), f64 CUDA-only (B), or per call (C)?
2. **Name / surface:** `gpu_map(xs, f)` (proposed — explicit, because the precision changes;
   foreman agrees), with `@gpu fn` for named kernels later?
3. **Permission default:** privileged (`--gpu` grant, CPU fallback without) as proposed?
4. **Backend order:** finish PTX (NVIDIA) first as proposed, or go straight to wgpu for
   portability (and crush-web)?
5. **Packed arrays:** `F32Array` only, or a general typed-array family (`i32s`, `f64s`) now?
