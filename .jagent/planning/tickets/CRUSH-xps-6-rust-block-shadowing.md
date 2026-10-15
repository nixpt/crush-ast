# CRUSH-xps-6 — Preserve or reject Rust block-local shadowing

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-6 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M2 / Rust parity |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-xps-3, CRUSH-xps-5 |
| **Estimated effort** | M |

## Problem

Crush VM locals are name-keyed within a function. The Rust walker currently
allows a nested `let` to overwrite an outer binding, so accepted Rust can print
the wrong value without an error. This is a slot collision, not the separate
CRUSH-251 captured-cell rule.

## Success criteria

- [ ] For `let x = 1; { let x = 2; println!("{}", x); } println!("{}", x);`,
      Rust-compatible execution prints `2` then `1`, or compilation rejects
      the shadowing clearly.
- [ ] No accepted fixture silently prints `2` then `2`.
- [ ] The parity harness classifies the old behavior under this ticket and
      removes the known-silent entry only after the fix is exact.
- [ ] Rust block-binding metadata remains available for closure capture; it is
      not conflated with the local-slot fix.

## Technical approach

- Alpha-rename nested block bindings in the Rust frontend, or reject a nested
  `let` that shadows an outer binding until unique slots are supported.
- Compare VM output with `rustc`; keep the AOT result exact-or-loud.

## Non-goals

- Changing Crush's local-storage representation.
- Implementing full Rust lexical scopes or closure capture.
