# CRUSH-35 — Walker-lowering completion: residual = typed arrays + VISION.md table refresh

| Field | Value |
|-------|-------|
| **ID** | CRUSH-35 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M6 |

## Problem

VISION.md's Walker Lowering Progress table shows 7 red items — but the table
is STALE. s412 triage verified 6 of 7 already closed in code: list
comprehensions (crush-lang-python/src/sdk.rs:180, PYLOWER-1), slices
(lower_expr.rs:253 + CRUSH-7's 8f30a96), tuple unpacking (lower_stmt.rs:523),
range() (lower_expr.rs:379 + compiler.rs:1489), Math.floor (CRUSH-65
a612b7f), fn-call-with-args (fe9a60a frame parity). **Residual real work:
typed arrays (Uint8Array)** — zero hits anywhere. Re-verify the 6 closures by
reading the cited sites before asserting them (they're triage claims, not
gospel).

## Approach

1. Typed arrays: decide representation (Value::Bytes exists since s298 —
   likely the backing store), lower Uint8Array construction/index/length from
   the JS walker; conformance fixtures.
2. VISION.md table refresh with per-item evidence commit/file:line — the
   table being wrong cost this milestone its shape once already.
3. Each of the 6 "closed" items gets a conformance-corpus fixture (CRUSH-73)
   so the table can't silently rot again.

## Definition of done

- [ ] Uint8Array fixture runs correctly through the real pipeline
- [ ] VISION.md table accurate with evidence per row
- [ ] 7 corpus fixtures committed (6 green + typed-array)

## Files in scope

- `crates/crush-lang-js` lowering, `crates/crush-vm` (only if Bytes indexing
  gaps), VISION.md

## Gates

None. Feeds CRUSH-103.
