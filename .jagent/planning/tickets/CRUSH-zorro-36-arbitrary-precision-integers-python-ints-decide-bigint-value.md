# CRUSH-zorro-36 — Arbitrary-precision integers (Python ints) — decide: bigint Value or loud overflow

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-36 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | VM |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |
| **Filed by** | claude (vega) — 2026-10-15, Python `.py` probe (50 programs vs python3, main `83f6599`) |

## Problem

Python ints are unbounded; crush's are `i64`. `10 ** 20` and `2 ** 100` fail with
`arithmetic overflow` (Probe programs: `/workspace/scratch/pyprobe/` (vega's box); run with `crush-walk-run <file>.py`.: `arith`, `bigint`) — loud, so not silently wrong, but common in
Python code (factorials, hashes, big literals).

## Decide

1. A `Value::BigInt` (e.g. `num-bigint`) that Int arithmetic promotes to on overflow, for Python-styled
   arithmetic only (JS numbers stay floats, Rust/C keep overflow semantics), or
2. keep i64 and document the loud overflow as a known difference.

Cost of (1): every arithmetic path in two engines + printing + AOT (loud there). Recommend deciding
after the higher-frequency Python gaps (CRUSH-zorro-28..34).
