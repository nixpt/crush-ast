# CRUSH-xps-3 — Add Rust-source compilation to crush-lang-sdk

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-3 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 / SDK |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-xps-2, CRUSH-197 |
| **Estimated effort** | M |

## Problem

The Rust frontend can lower a subset of Rust to CAST, but a Rust application
cannot call a production SDK API to compile that source into a VM program. The
existing `crush-lang-rust/src/sdk.rs` helper is test-only, manually duplicates
pipeline work, and must not become the public API unchanged.

## Success criteria

- [ ] `crush-lang-sdk` exposes feature-gated `compile_rust_source(&str)` that
      returns the same VM program type used by `Runtime::run`.
- [ ] Rust source compilation uses the existing Rust frontend and common
      compiler path; execution uses ordinary `Runtime`, quotas, and explicit
      host capabilities.
- [ ] The crate dependency graph remains acyclic; the current SDK dev-dependency
      from `crush-lang-rust` is removed or the integration is placed in a bridge
      crate with a documented reason.
- [ ] SDK tests compile and run representative Rust corpus fixtures, including
      output and return behavior where supported.
- [ ] Disabled feature builds do not pull in the Rust frontend.

## Technical approach

- Add an optional dependency/feature or a small bridge crate after checking the
  Cargo test dependency graph.
- Route Rust CAST through the existing `crush-frontend` and SDK conversion
  helpers; do not invent a second VM/runtime.
- Move end-to-end Rust execution tests to the SDK integration layer and retire
  the test-only helper once equivalent coverage exists.

## Non-goals

- Changing the supported Rust subset or invoking `rustc`/Cargo.
- Automatically granting host capabilities based on Rust source.
