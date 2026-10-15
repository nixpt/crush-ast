# CRUSH-xps-5 — Gate Rust execution on exact-or-loud parity

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-5 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M2 / conformance gate |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-xps-3 |
| **Estimated effort** | M |

## Problem

The Rust frontend currently has silently mis-lowered constructs. Exposing `.rs`
through the default runner before these cases are exact or rejected would make
incorrect behavior look supported. This phase is the mandatory gate before
CRUSH-xps-4 enables Rust in `crush run`.

## Success criteria

- [ ] VM fixtures compare exactly with `rustc`; the freshness test re-runs
      `rustc` so checked-in expected output cannot silently drift.
- [ ] AOT fixtures compare exactly or fail loudly; add
      `crates/crush-aot/tests/rust_parity.rs`.
- [ ] A known-loud allow-list verifies expected failures and fails with a
      "now exact — remove it" message when parity is fixed.
- [ ] Known-silent cases are keyed to fixing tickets; an unlisted silent
      divergence fails the audit.
- [ ] Unsupported syntax reports source location, construct, and current
      limitation; unsupported constructs never lower to a no-op.
- [ ] The accepted subset and material semantic differences are documented;
      parser acceptance alone does not count as support.
- [ ] Initial coverage/fixes include block shadowing (alpha-rename per-block
      `let`s or reject loudly), `Vec`/`String`/struct value semantics
      (`clone`, assignment, argument passing), integer widths/`as`/wrapping/
      overflow, and `{:?}` formatting.

## Technical approach

- Copy the exact-or-loud harness shape used by the Python and JS parity work.
- Extend the existing rustc-backed corpus and add the AOT parity twin.
- Alpha-rename block-local bindings or reject shadowing; do not treat the
  CRUSH-251 captured-cell rule as the fix for VM slot collisions.
- Keep unsupported-feature tests fail-closed and remove each known-silent entry
  only when the VM and AOT behavior is exact.

## Non-goals

- Full Rust type checking or arbitrary Rust/Cargo compatibility.
- Enabling `.rs` in the ordinary runner before this ticket is complete.
