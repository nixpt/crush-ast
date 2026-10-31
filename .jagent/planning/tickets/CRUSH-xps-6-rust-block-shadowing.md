# CRUSH-xps-6 — Preserve or reject Rust block-local shadowing

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-6 |
| **Priority** | P1 |
| **Status** | In review |
| **Phase** | M2 / Rust parity |
| **Assignee** | nixp (user-directed) |
| **Dependencies** | CRUSH-xps-3, CRUSH-xps-5 |
| **Estimated effort** | M |

## Problem

Crush VM locals are name-keyed within a function. The Rust walker currently
allows a nested `let` to overwrite an outer binding, so accepted Rust can print
the wrong value without an error. This is a slot collision, not the separate
CRUSH-251 captured-cell rule.

## Success criteria

- [x] For `let x = 1; { let x = 2; println!("{}", x); } println!("{}", x);`,
      Rust-compatible execution prints `2` then `1`, or compilation rejects
      the shadowing clearly.
- [x] No accepted fixture silently prints `2` then `2`.
- [x] The parity harness classifies the old behavior under this ticket and
      removes the known-silent entry only after the fix is exact.
- [x] Rust block-binding metadata remains available for closure capture; it is
      not conflated with the local-slot fix.

## Technical approach

- Alpha-rename nested block bindings in the Rust frontend, or reject a nested
  `let` that shadows an outer binding until unique slots are supported.
- Compare VM output with `rustc`; keep the AOT result exact-or-loud.

## Non-goals

- Changing Crush's local-storage representation.
- Implementing full Rust lexical scopes or closure capture.

## Implementation and validation

- A private Rust-only post-lowering pass resolves each local declaration to a
  fresh CAST name. This includes initializer ordering, callable variables,
  parameters, branches, loop bindings/bodies, matches, and closure bodies.
- The prefix is absent from source, so source identifiers and inline format
  captures cannot collide with generated storage. Names remain valid for
  lifted closures and every existing engine.
- Function catalogs and variable/declaration metadata retain source name,
  function, binding ID, scope ID, and kind. This is independent of VM slots
  and dynamic captured-cell lifetime.
- Source calls are distinguished from compiler helpers: a callable variable
  named `__crush_assign__` does not intercept assignment lowering; locals named
  `print`/`println` override legacy function-form print mappings. Capability
  names remain resolved globals.
- Removed `audit_block_shadowing` from the shared known-silent policy after VM
  and both AOT backends became exact. Expanded the rustc corpus to 37 fixtures,
  with eight new binding cases; closure cases remain explicitly loud on AOT.
- Nested function items now fail at their source location until item hoisting
  and block-local function lookup are implemented. This is documented as a
  supported-subset boundary, not silently approximated.
- Fresh affected-package rebuild in the dedicated
  `/workspace/projects/.codex-target-crush-xps6` target (isolated from parent
  worktree caches): SDK/Rust unit and integration suite on main `3d512af`,
  580 passed.
- VM corpus freshness, both AOT audits, C GC stress, and scoped clippy passed.
- Independent Rust SDK host matched freshly compiled rustc output for the
  original block (`2\n1\n`), branches/loops, matches, callable shadowing,
  captures, and intrinsic-like source names. A .crush program still printed
  `Crush from Rust` and returned 42.

Review: [PR #224](https://github.com/nixpt/crush-ast/pull/224), based on
CRUSH-xps-5 / PR #220.
Rust value/copy/borrow semantics remain xps-7 work; this ticket does not alter
Crush's local-storage representation.
