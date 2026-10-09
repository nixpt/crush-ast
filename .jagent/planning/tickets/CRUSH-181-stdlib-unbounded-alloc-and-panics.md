# CRUSH-181 — Default-on stdlib: unbounded allocations abort the host; negative/invalid args panic

| Field | Value |
|-------|-------|
| **ID** | CRUSH-181 |
| **Priority** | P0 |
| **Status** | Backlog |
| **Phase** | M7 |
| **Assignee** | unassigned |
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

- [ ] No stdlib call can panic on any argument value; negatives and bad radixes give
      a runtime error.
- [ ] A per-run memory / max-string-size quota (`--max-memory` or reuse an
      existing quota) applied to every allocation-growing cap and to `+`.
- [ ] Fuzz-style test feeding boundary ints (`-1`, `0`, `i64::MAX`) to each stdlib cap.
