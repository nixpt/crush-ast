# CRUSH-zorro-52 — Auto GPU offload of built-in ops (VEC_ADD/VEC_DOT/MAT_MUL, stdlib sort/reduce/scan) above a size threshold

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-52 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | VM / M5 (backends) |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-zorro-53 (silent coercion — fix first), CRUSH-zorro-51 (shared GPU runtime), CRUSH-zorro-3 |
| **Estimated effort** | L |
| **Filed by** | claude (vega) — 2026-10-15, GPU viability follow-up (owner request) |

## Problem

Programs that never write a kernel could still use the GPU if the VM's own heavy built-ins ran there
when the input is big enough: `MAT_MUL`, `VEC_DOT`, `VEC_ADD` (crush-vm `scheduler.rs:815-…`,
mirrored in the portable VM), and stdlib collection work (sort, sum/min/max, prefix scan,
histograms; later RNG and FFT). No new syntax; existing code benefits.

## What the measurements say (2026-10-15, RTX 5070 Ti)

Offloading only pays when the work per byte copied is high (memory-bound maps lost to one CPU
thread end to end: copies dominate). So:

- **`MAT_MUL`** — O(n³) work on O(n²) data: the prime candidate (above ~256×256).
- **sort** (large), **scan/histogram** on large inputs: likely wins, to be measured.
- **`VEC_ADD`, `VEC_DOT`, `sum`** — memory-bound: one-shot offload **loses**; worth it only for data
  already resident on the device (a later residency ticket). Don't offload them per call.

## Prerequisite

CRUSH-zorro-53: `VEC_ADD` / `VEC_DOT` / `MAT_MUL` silently turn non-numbers into `0.0` and truncate
mismatched lengths — filed separately (it is wrong today, offload or not) and fixed first.

## Design constraints

- **Identical results, CPU or GPU.** Built-ins compute in f64 today. Keep f64 — don't silently
  drop to f32 (that is `gpu_map`'s explicit choice, not a built-in's). But consumer GPUs run f64 at
  ~1/64 rate (the 5070 Ti included), so the GPU only wins above a size where it beats a blocked f64
  GEMM on all CPU cores — **measure the crossover per box and device** (a calibration run cached per
  device), never a fixed constant, or auto-offload makes some programs slower.
- **Reductions** (`VEC_DOT`, `sum`, `MAT_MUL` inner products): summation order changes the result.
  Define one deterministic order (blocked pairwise tree) and use it on the CPU path too, so both
  give the same bits. That changes today's f64 results in the last bits for existing programs: a
  deliberate behaviour change — changelog entry, corpus pins updated in the same PR.
- **Never without the grant:** implicit GPU use without the `gpu` grant would be a silent
  permission escalation. Without it the CPU path runs, with identical results. `CRUSH_GPU=off`
  forces the CPU; `gpu.backend()` reports which ran.
- Runs through the shared GPU runtime proposed in CRUSH-zorro-51 (backend trait, module cache,
  typed buffers); built-in kernels ship precompiled (PTX/WGSL text in the crate), not built per call.

## Done when

- [ ] The prerequisite fixes land (loud type/length errors).
- [ ] `MAT_MUL` offloads above a measured threshold with bit-identical results to the CPU path
      (same reduction order), with a benchmark showing the crossover size.
- [ ] Each further op (sort, scan, …) added only with its measured crossover and an identity test.
