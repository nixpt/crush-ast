# CRUSH-zorro-39 — parallel_map: freeze captured values in workers (O(1) write detection)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-39 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | VM |
| **Assignee** | unassigned |
| **Dependencies** | #207 (parallel_map) |
| **Estimated effort** | M |
| **Filed by** | claude (vega) — 2026-10-15, foreman review of #207 |

## Problem

`parallel_map` (#207) moves a closure's captured values into each worker thread once and, after
every item, deep-compares them with the snapshot to catch a write (which would be lost — workers
share nothing). That is correct and now linear (1600 items × a 200k captured array: 1.1 s, was
minutes), but the check is still O(|captures|) per item, and the error can't say *where* the write
happened.

## Proposal

Freeze captured values inside a worker: captured cells, arrays and maps are marked read-only, and
every mutation path in both VMs (CELL_SET, ARR_SET / ARR_PUSH / ARR_POP, map set, in-place
capabilities) fails immediately on a frozen value with the existing "changed something it captured"
message plus the instruction / source location. O(1) per write, nothing to compare, can't miss a
write. Needs a frozen flag reachable from the `Rc` (or a per-thread set of frozen pointers checked
on mutation).

## Done when

- [ ] A write to a captured value in a worker fails at the write, naming the location.
- [ ] `Worker::check`'s per-item compare is gone; the scaling test still holds.
