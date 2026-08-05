# CRUSH-79 — casm DebugInfo.source_map: flat vector returns wrong function's location

| Field | Value |
|-------|-------|
| **ID** | CRUSH-79 |
| **Priority** | P1 |
| **Status** | Done |
| **Phase** | Correctness spine (s412) |

## Problem

Panini capture (2026-08-02, dejavue `a901a22`): `record_debug_info_for_function`
appends each function's per-function pc entries into ONE flat vector, so
`source_location_for_pc` returns the wrong function's location for any
multi-function program (evidence: crush-frontend `compiler.rs:312-319` +
`casm/src/debug_info.rs:166-168`; re-verify at dispatch). This is the far-end
bug of the span wire CRUSH-74 connects — fixing 74 without this yields
confidently-wrong locations, which is worse than none.

## Approach

Key the map per function (function id/index + pc), or offset pcs into a global
program pc-space consistently at record time. Add a two-function test:
locations resolve to the correct function for pcs in each.

## Resolution

Landed on `agent/panini-crush/CRUSH-79` (`a5bab63`). Chose the offset approach:
keep the flat `source_map: Vec<SourceLocation>` for backward compat, add
`fn_offsets: HashMap<String, (usize, usize)>` mapping function names to
(start, end_exclusive) ranges.

Changes (4 files, +122/-5):
1. **`debug_info.rs`**: Added `fn_offsets` field, `record_function_range()`,
   and `source_location_for_function_pc()` with end-boundary check.
2. **`compiler.rs`**: `record_debug_info_for_function` now records the
   function's range after pushing all its instructions.
3. **`lib.rs`**: `format_runtime_error_with_location` accepts optional
   `fn_name` parameter; uses per-function lookup when provided.
4. **`source_map_tests.rs`**: Multi-function test verifies correct function
   resolution for "main" vs "helper" function pcs.

**Gates CRUSH-74** — the far-end bug is now fixed; CRUSH-74's end-to-end test
(item 4 in its DoD) can now correctly assert multi-function source locations.

## Definition of done

- [x] Multi-function location lookup test green (would fail before)
- [x] `cargo test -p casm -p crush-frontend` green (223 tests total)
- [x] Coordinated with CRUSH-74 (same agent, same session — CRUSH-74 up next)

## Files in scope

- `crates/casm/src/debug_info.rs`, `crates/crush-frontend/src/compiler.rs`

## Gates

None. Pairs with CRUSH-74.
