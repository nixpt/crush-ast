# CRUSH-xps-2 — Phased Rust ↔ Crush pipeline design

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-2 |
| **Priority** | P1 |
| **Status** | Done — design PR #204 merged 2026-10-15 |
| **Phase** | M0 / design |
| **Assignee** | nixp |
| **Dependencies** | CRUSH-266 |
| **Estimated effort** | S |

## Problem

Rust-to-Crush translation, Rust-hosted Crush execution, and native Rust plugin
calls exist at different maturity levels. Without a phased contract, future
work can conflate a supported Rust subset with arbitrary Cargo execution or
make package code responsible for native-code grants.

## Success criteria

- [x] The design names the current baseline and gaps for `.rs`, `.crush` in a
      Rust host, and Rust crate wrappers.
- [x] Follow-up tickets separate the SDK API, CLI source dispatch, and Rust
      conformance/diagnostics work.
- [x] The manual, host-approved wrapper MVP remains the current Rust crate
      integration path.
- [x] Cargo-managed builds are explicitly deferred behind CRUSH-179 and a
      separate trust/reproducibility design.
- [ ] The design PR is reviewed and merged.

## Technical approach

- Record the phase plan in `.jagent/planning/designs/rust-crush-pipeline.md`.
- Use CRUSH-xps IDs for this host's follow-up work.
- Preserve the Rust frontend → CAST → CASM → VM boundary and the SDK's
  explicit `Runtime` / `HostCaps` boundary.

## Non-goals

- Implementing any runtime or compiler changes in this design PR.
- Enabling arbitrary Rust or Cargo execution from Crush.

## Survey 2026-10-31 (status)

PR #204 (`agent/nixp/CRUSH-xps-2`) merged 2026-10-15, which is the one unticked box ("design PR is reviewed and merged"). Previous status: `In Review`.
