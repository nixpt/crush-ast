# CRUSH-xps-3 — Add a shared walker-backed compile API to crush-lang-sdk

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

The SDK's production compile API accepts Crush source directly, while other
source frontends remain disconnected from a common host-facing path. This
encourages Rust-specific helpers and per-language dispatch duplication. The
existing `crush-lang-rust/src/sdk.rs` helper is test-only, manually duplicates
pipeline work, and must not become the public API unchanged.

## Success criteria

- [ ] `crush-lang-sdk` exposes feature-gated
      `compile_source(language_or_extension, source)` over the existing
      `WalkerAdapter` registry and returns the VM program type used by
      `Runtime::run`.
- [ ] `compile_rust_source(&str)`, if retained, is a thin convenience wrapper
      over the registry-backed API.
- [ ] Per-language adapters are individually feature-gated; disabling a
      language does not pull in its frontend dependencies.
- [ ] All selected languages use the common compiler path; execution uses
      ordinary `Runtime`, quotas, and explicit host capabilities.
- [ ] The crate dependency graph remains acyclic; the current SDK dev-dependency
      from `crush-lang-rust` is removed or the integration is placed in a bridge
      crate with a documented reason.
- [ ] SDK tests compile and run representative Rust corpus fixtures, including
      output and return behavior where supported.
- [ ] Disabled feature builds do not pull in the Rust frontend.

## Technical approach

- Add an optional dependency/feature or a small bridge crate after checking the
  Cargo test dependency graph.
- Reuse `WalkerAdapter` and the extension lookup in `crush-walker-core`; do
  not add a Rust-only frontend table.
- Route each adapter's CAST through the existing `crush-frontend` and SDK
  conversion helpers; do not invent a second VM/runtime.
- Move end-to-end Rust execution tests to the SDK integration layer and retire
  the test-only helper once equivalent coverage exists.

## Non-goals

- Changing the supported Rust subset or invoking `rustc`/Cargo.
- Automatically granting host capabilities based on Rust source.
