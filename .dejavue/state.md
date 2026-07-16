# State

Updated: 2026-07-15T23:50:23-05:00

AOT pipeline (crush-aot) verified working end-to-end for the first time via bozo's benchmark verification pass: RuntimeValue::Str bug (blocked 100% of AOT-Rust compiles) found+fixed, Math.floor case-mismatch in JS lowering found (lower_swc.rs emits 'Math.floor', compiler.rs only matches lowercase 'math.floor' -- NOT fixed yet). LTO/strip binary-size claim in readiness-matrix.md corrected: measured 33% on crush-aotc itself (stripping accounts for most of it), not the documented 64-80%.
