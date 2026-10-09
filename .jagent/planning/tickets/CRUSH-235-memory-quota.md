# CRUSH-235 — No memory budget: `Quotas` limits steps, stack, output and depth but not heap

| Field | Value |
|-------|-------|
| **ID** | CRUSH-235 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M7 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-41 |
| **Estimated effort** | M |
| **Filed by** | claude — 2026-10-09 use-case review (agents + polyglot), on `main` `2d71b96` |

## Problem

`Quotas` has `max_steps`, `max_stack`, `max_output`, `max_call_depth` and a wall clock
(`crates/crush-vm/src/vm.rs:~694-731`) but nothing bounds memory. An agent-written loop
that grows an array or string exhausts host memory instead of hitting a quota. (CRUSH-41
is instruction fuel, not memory.)

## Success criteria

- [ ] `Quotas::max_heap_bytes` (or element/byte count) enforced by the VMs on allocation
      of strings, arrays and maps; exceeding it is a `QuotaExceeded` runtime error.
- [ ] `crush-run --max-memory`; a test where a growing loop stops with the quota error.
