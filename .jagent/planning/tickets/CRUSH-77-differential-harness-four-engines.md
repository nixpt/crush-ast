# CRUSH-77 — Differential harness: same program, all engines, identical results

| Field | Value |
|-------|-------|
| **ID** | CRUSH-77 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | Correctness spine (s412) |

## Problem

crush-ast has four execution engines (PortableVm ~83 opcode arms; FastVm with
its own ~86-variant FastOp enum + arena value model; crush-jit via Cranelift;
crush-aot C codegen) with divergent value models and opcode sets, and no test
asserting the same program yields the same result on all of them. This is the
setup Rustlantis exploited to find 22 rustc/LLVM miscompiles, and Cranelift's
own fuzzgen target does exactly this compile-and-compare. The reference
interpreter exists for free.

**Partially built — scope only the delta (verify at dispatch):** a `crush-diff`
harness exists (`crush-lang-sdk/differential.rs` + the `differential_aot` test
suite; CRUSH-13 and CRUSH-25 both extended it). Establish exactly which engine
pairs and which programs it covers today; the gap per July research was uniform
four-way coverage.

## Approach

1. One entry: program source (or corpus file from CRUSH-73) → run on
   PortableVm, FastVm, JIT, AOT-C → assert identical stdout + exit.
2. JIT lane depends on CRUSH-72 (bail-out): a refused function falls back and
   still yields a comparable result rather than a fabricated null.
3. Known-nondeterminism (HashMap iteration order, CRUSH-25 residue) will
   flake this — either run point-fixes or gate full enablement on CRUSH-42
   (deterministic mode); document which.
4. Wire the CRUSH-73 corpus through it once both exist.

## Definition of done

- [ ] Four-way differential run over ≥20 programs green in CI (or explicitly
      xfailed with ticket refs)
- [ ] Coverage statement: which engines × which opcode families are compared
- [ ] At least the divergences already known (Math.* class, CRUSH-65/69) have
      regression entries

## Files in scope

- `crates/crush-lang-sdk/src/differential.rs` (or its current home), test suites

## Gates

CRUSH-72 for the JIT lane; CRUSH-42 de-flakes. Gates CRUSH-59.
