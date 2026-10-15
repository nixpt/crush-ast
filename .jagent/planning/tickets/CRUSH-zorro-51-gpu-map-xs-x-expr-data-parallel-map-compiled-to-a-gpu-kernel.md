# CRUSH-zorro-51 — gpu_map(xs, |x| expr): data-parallel map compiled to a GPU kernel, with an identical CPU fallback

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-51 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M5 (backends) / VM |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-zorro-3 (gpu.launch, done), #207 parallel_map, CRUSH-zorro-50 |
| **Estimated effort** | L |
| **Filed by** | claude (vega) — 2026-10-15, GPU viability test on the RTX 5070 Ti |

## Problem

Crush can run a GPU kernel today, but only one written by hand at the device level
(CRUSH-zorro-3): `gpu.global_id()`, `gpu.load_f32` / `gpu.store_f32`, an explicit `gpu.launch`
with a thread count, f32 buffers copied in and out, NVIDIA only (cudarc), and no way to run the
same program without a GPU. Data-parallel numeric work — the case where the GPU pays off
(measured 2026-10-15 on an RTX 5070 Ti: a 64-FMA map over 16M elements, 12 ms with copies vs
21 ms on 32 CPU threads, ~26 s through `parallel_map`, ~350 s on one crush VM) — should be one
call, like `parallel_map`.

## Proposal

`gpu_map(xs, |x| expr)` — see the design: `.jagent/planning/research/2026-10-15-gpu-map-design.md`
(kernel subset, f32-vs-f64 decision, typed buffers, CPU fallback with identical results, PTX now and
WGSL/wgpu next, phases, open questions for the owner).

## Done when

- [ ] Design agreed (owner decisions: precision, API name, permission default, backend order).
- [ ] Phases 1–3 of the design: `gpu_map` runs on the GPU via the CRUSH-zorro-3 runtime and,
      without a GPU, on the CPU fallback with **bit-identical** results; non-kernel code is a
      compile error naming the construct.
