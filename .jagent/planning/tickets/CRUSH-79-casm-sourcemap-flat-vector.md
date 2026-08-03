# CRUSH-79 — casm DebugInfo.source_map: flat vector returns wrong function's location

| Field | Value |
|-------|-------|
| **ID** | CRUSH-79 |
| **Priority** | P1 |
| **Status** | Backlog |
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

## Definition of done

- [ ] Multi-function location lookup test green (would fail before)
- [ ] `cargo test -p casm -p crush-frontend` green
- [ ] Coordinated with CRUSH-74 (same dispatch or explicit ordering note)

## Files in scope

- `crates/casm/src/debug_info.rs`, `crates/crush-frontend/src/compiler.rs`

## Gates

None. Pairs with CRUSH-74.
