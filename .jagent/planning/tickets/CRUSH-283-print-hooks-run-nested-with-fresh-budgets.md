# CRUSH-283 — VM: str/repr print hooks (`render_hooked`) still run nested with fresh budgets

| Field | Value |
|-------|-------|
| **ID** | CRUSH-283 |
| **Priority** | P2 |
| **Status** | Open |
| **Phase** | COSM follow-up (post CRUSH-277) |
| **Assignee** | unassigned |
| **Filed by** | muse — 2026-10-20, from kai's re-review of PR #285 |

## Problem

CRUSH-285 review #1 moved COSM accessors (getters/setters) onto the
caller's thread: they now run as ordinary calls charged to the
caller's step and call-depth quotas, and their throws reach the
program's `try`/`catch`. The str/repr print hooks did not move:
`render_hooked` in both VMs (`crates/crush-vm/src/vm.rs`,
`crates/crush-vm/src/portable_vm.rs`) still runs the hook in a nested
VM (`run_from` / a fresh `PortableVm`) with fresh budgets.

A print hook that loops or recurses therefore escapes the program's
step/call-depth quotas the same way accessors did before the fix, and
a hook's throw bypasses the program's exception path.

## Scope notes

- JS (boa walker) does not emit print hooks today, so no JS input
  reaches this; it is reachable from crush-language programs whose
  kinds declare str/repr hooks (K1.4).
- The fix shape should mirror the accessor fix: run the hook as an
  ordinary call on the same thread (frame with a return disposition
  that replaces the rendered value), rather than a nested run.
- Disclosed on PR #285 (CRUSH-277) so the remaining nested-run path
  is tracked rather than implicit.
