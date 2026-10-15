# CRUSH-xps-5 — Strengthen Rust subset conformance and diagnostics

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-5 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M3 / conformance |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-xps-3 |
| **Estimated effort** | M |

## Problem

The Rust frontend intentionally supports only a subset, but parity fixtures and
diagnostics need to make the accepted boundary dependable as the subset grows.

## Success criteria

- [ ] Every accepted fixture has expected behavior checked against equivalent
      `rustc` execution where the semantics overlap.
- [ ] Unsupported syntax reports source location, construct, and current
      limitation.
- [ ] Tests cover silent-drop regressions for top-level declarations, imports,
      qualified paths, and unsupported expressions.
- [ ] Documentation lists accepted features and material semantic differences.
- [ ] No feature is claimed as supported based only on parser acceptance.

## Technical approach

- Extend the existing rustc-backed corpus by feature family.
- Carry source spans from `syn` into frontend diagnostics.
- Keep unsupported-feature tests fail-closed and avoid accepting syntax that
  lowers to a no-op.

## Non-goals

- Full Rust type checking or arbitrary Rust/Cargo compatibility.
- Broadening the Rust subset without a concrete parity fixture.
