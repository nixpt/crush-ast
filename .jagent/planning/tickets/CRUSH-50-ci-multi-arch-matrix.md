# CRUSH-50 — CI multi-arch matrix (aarch64 + riscv64 via cross)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-50 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M8 |

## Problem

Zero arch-specific CI: no aarch64/riscv64 lanes (verify ci.yml). There is no
arch-specific code anywhere yet (July note) — which is exactly why cheap
cross-compile lanes now prevent expensive surprises when M8's Pi/Android/wasm
work starts.

## Approach

`cross`-based compile (+ test under qemu where feasible, else compile-only,
stated explicitly) for `aarch64-unknown-linux-gnu` + `riscv64gc-unknown-linux-gnu`.
AOT comparability item from ROADMAP scoped honestly: assert AOT-C *C source*
output is arch-independent (bit-for-bit native .so across arches is not a
meaningful goal — record that reframing in the ticket close). Same cache-key
discipline as CRUSH-49.

## Definition of done

- [ ] Both cross lanes live (compile-only clearly labeled if so)
- [ ] AOT-C source-level arch-independence asserted by test
- [ ] ort/onnxruntime cross-compile story checked (the CRUSH-26 dep may not
      ship riscv binaries — feature-gate per-arch if needed, documented)

## Files in scope

- `.github/workflows/ci.yml` (workflow-scope push caveat as CRUSH-49)

## Gates

CRUSH-49.
