# CRUSH-373 — `crush-run`: preserve float literals exactly when reading CAST JSON

| Field | Value |
|-------|-------|
| **ID** | CRUSH-373 |
| **Priority** | P1 |
| **Status** | Done — merged in PR #406 (`79859626`, 2026-11-07); see Resolution |
| **Phase** | Maya audit |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | altair — 2026-11-06, from satya's Maya-through-Crush audit |

## Problem

Float literals change value when a `.cast.json` is run through `crush-run`, with no error. The audit measured 335 of 4492 literals (7.5%) drifting by about 1 ULP, and one JS driver differed from node on 215 of 1500 lines this way. The walker door shows no changes.

Measured at `main` `495d9995` (v0.11.1) by the 2026-11-06 Maya-through-Crush audit.

## Evidence

400 random literals printed through `python_walker` then `crush-run run`: 49 changed value compared with CPython, for example `0.9097040631431023` printed as `0.9097040631431024`. No `Cargo.toml` in the workspace enables serde_json's `float_roundtrip`. That is the likely cause, not a proven one. Re-run independently on `495d9995` through `python_walker` then `crush-run run`.

## Success criteria

- [x] A literal survives CAST JSON write and read with identical bits
- [x] A test covers a large random literal set through the `crush-run` path

## Resolution

PR #406 (altair) proved the cause: serde_json's default float parser doesn't guarantee round-trips. The fix enables `float_roundtrip` on the workspace `serde_json` dependency. The CAST writer was already exact.

- `crates/crush-lang-sdk/tests/cast_float_roundtrip.rs` runs 1000 sampled literals through the real `crush-run` binary. It failed with 81 changed before the fix and passes after.
- 400 random literals through `python_walker` then `crush-run run` match CPython, with 0 changed against 49 before.
- **Cost:** that parser is slower on floats. This wasn't measured, and only JSON reading is affected.
