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
- [ ] Initial known-silent entries link to CRUSH-xps-6 through CRUSH-xps-12;
      `.rs` CLI work stays blocked until those tickets close.
- [ ] Unsupported syntax reports source location, construct, and current
      limitation; unsupported constructs never lower to a no-op.
- [ ] The accepted subset and material semantic differences are documented;
      parser acceptance alone does not count as support.
- [ ] Initial corpus includes block shadowing, `Vec`/`String`/struct value
      semantics, integer widths/casts/overflow, and `{:?}`; fixing these
      families is tracked separately in CRUSH-xps-6 through CRUSH-xps-12.

## Technical approach

- Copy the exact-or-loud harness shape used by the Python and JS parity work.
- Extend the existing rustc-backed corpus and add the AOT parity twin.
- Keep the known-silent inventory linked to feature-fix tickets; do not mark
  those entries exact until their VM and AOT parity proofs pass.
- Keep unsupported-feature tests fail-closed and remove each known-silent entry
  only when the VM and AOT behavior is exact.

## Non-goals

- Full Rust type checking or arbitrary Rust/Cargo compatibility.
- Enabling `.rs` in the ordinary runner before this ticket is complete.
