# CRUSH-181 — Default-on stdlib: unbounded allocations abort the host; negative/invalid args panic

| Field | Value |
|-------|-------|
| **ID** | CRUSH-181 |
| **Priority** | P0 |
| **Status** | Done |
| **Phase** | M7 |
| **Assignee** | nixp |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

Stdlib caps are registered by default, but have no size bound and a few use
unchecked casts, so a program can abort the host process (exit 134/101) instead
of getting a runtime error.

## Reproduction

| Program | Result |
|---|---|
| `io.print(str.repeat("ab", -1))` | panic: capacity overflow (`get_int(..) as usize`) |
| `str.pad_left("x", -1, " ")` / `pad_right` | same cast |
| `conv.parse_int("10", 1)` | panic in `from_str_radix` (radix outside 2..=36) |
| `str.repeat("a", 4000000000)` | allocation failure → abort |
| `buffer.alloc(9000000000000)` | tries to allocate ~288 TB → abort |
| doubling `s = str.concat(s, s)` 26× | 1 GB allocation, abort |

Only the `+` operator is bounded (by `max_output`, which is itself wrong — see CRUSH-186).

## Where

`crates/crush-vm/src/stdlib.rs` ~302/314/327 (pad/repeat casts), ~624 (radix);
`buffer.*`, `str.concat`, `str.join`.

## Success criteria

- [x] No stdlib call can panic on any argument value; negatives and bad radixes give
      a runtime error.
- [x] A per-run memory / max-string-size quota (`--max-memory` or reuse an
      existing quota) applied to every allocation-growing cap and to `+`.
      *(A fixed 64 MiB bound; a configurable `--max-memory` quota is a follow-up
      — the `HostCap` ABI has no run context to read a quota from.)*
- [x] Fuzz-style test feeding boundary ints (`-1`, `0`, `i64::MAX`) to each stdlib cap.

## Resolution (2026-10-30, `agent/nixp/CRUSH-296`)

The stdlib caps live in `crates/crush-lang-sdk/src/stdlib.rs` (and
`stdlib/bytes.rs`) now, not `crush-vm`.

- `str.repeat`, `str.pad_left`, `str.pad_right` and `collections.chunk` now
  reject negative lengths/counts (they used to cast to `usize`, wrapping to a
  huge value) and refuse a result over `MAX_STDLIB_ALLOC_BYTES` (64 MiB) — the
  `checked_mul` on `str.repeat` also catches a size that overflows `usize`.
- `conv.parse_int` validates the radix (2..=36) instead of panicking inside
  `from_str_radix`.
- `buffer.alloc` refuses a size whose allocation would exceed the bound.
- `str.join` bounds its result before allocating it.

### Evidence

- `crates/crush-lang-sdk/src/stdlib.rs::tests::stdlib_caps_reject_negative_and_huge_arguments`
  feeds `-1`, `i64::MIN`, `i64::MAX`, `4_000_000_000` and bad radixes to every
  affected cap and requires a runtime error, then checks the valid calls still
  work. Verified RED (a reverted `str.repeat` panics with "capacity overflow").
- Live: `crush-run` now prints `[runtime] … count must be >= 0 …`,
  `… radix must be 2..=36 …`, `… over the 67108864-byte limit` instead of
  aborting.
- `cargo test --workspace --lib --tests`: 2540 passed, 0 failed.

## Follow-up

A configurable per-run `--max-memory` quota (rather than the fixed 64 MiB
bound) needs the `HostCap` call path to carry the run's `Quotas`; today
`HostCap::call(&self, args)` has no context. Separate ticket.
