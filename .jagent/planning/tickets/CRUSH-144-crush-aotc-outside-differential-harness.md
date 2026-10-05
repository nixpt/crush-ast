# CRUSH-144 — crush-aotc is outside the differential harness (missed #76 string ordering)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-144 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

`crush-aotc` (CASM → C, NaN-boxed `CrushValue`) is a separate backend from `crush-aot`'s C emitter, and `crush-aot/tests/differential_aot.rs` doesn't run it. So language decisions applied "on every backend" skip it silently:

- #76 / CRUSH-136: `CV_CMP_LT`/`GT`/`LE`/`GE` still call `cv_require_numeric`, so `"a" < "b"` aborts. `crates/crush-aotc/tests/integration.rs::ordered_comparison_on_string_is_rejected` still pins the old behaviour.
- CRUSH-134 truthiness was aligned by hand (found while grepping), not by a failing differential test.

## Success criteria

- [ ] crush-aotc runs in the differential harness (at least for programs it accepts), so a divergence fails CI
- [ ] Two strings order by code point in crush-aotc (`strcmp` on unsigned bytes); `ordered_comparison_on_string_is_rejected` pins the new behaviour

## Files to modify

- `crates/crush-aotc/src/rt_header.rs`, `crates/crush-aotc/tests/integration.rs`, `crates/crush-aot/tests/differential_aot.rs`
