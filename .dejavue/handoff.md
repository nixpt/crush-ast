# Handoff

Updated: 2026-07-15T23:50:23-05:00

## Summary
Verified crush-aotc's benchmark + LTO claims empirically for nixpt/bozo's design work; fixed one real, 100%-reproducible AOT-Rust codegen bug (RuntimeValue::Str vs the enum's actual String variant, commit 5f30520 / c27601e on origin/main).

## Next Steps
Fix the Math.floor (and likely Math.max/min/pow/etc) case-mismatch between lower_swc.rs's JS-style capitalized names and compiler.rs's lowercase math.* builtin table -- silently miscompiles today (165 instead of 465 on docs/benchmarks/compute.js). Audit lower_swc.rs<->compiler.rs and codegen.rs<->codegen_c.rs for the same double-maintained-table bug class.

## Boot Instructions
Read `.dejavue/handoff.md`, `.dejavue/state.md`, `.dejavue/decisions.md`, and `.dejavue/timeline.jsonl` before making changes.
