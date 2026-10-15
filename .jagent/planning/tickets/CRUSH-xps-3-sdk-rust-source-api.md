# CRUSH-xps-3 — Add a shared walker-backed compile API to crush-lang-sdk

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-3 |
| **Priority** | P1 |
| **Status** | In review |
| **Phase** | M1 / SDK |
| **Assignee** | nixp |
| **Dependencies** | CRUSH-xps-2, CRUSH-197, CRUSH-xps-10, CRUSH-xps-11 |
| **Estimated effort** | M |

## Problem

The SDK's production compile API accepts Crush source directly, while other
source frontends remain disconnected from a common host-facing path. This
encourages Rust-specific helpers and per-language dispatch duplication. The
existing `crush-lang-rust/src/sdk.rs` helper is test-only, manually duplicates
pipeline work, and must not become the public API unchanged.

## Success criteria

- [x] `crush-lang-sdk` exposes feature-gated
      `compile_source(language_or_extension, source)` over the existing
      `WalkerAdapter` registry and returns the VM program type used by
      `Runtime::run`.
- [x] `compile_rust_source(&str)`, if retained, is a thin convenience wrapper
      over the registry-backed API.
- [x] Per-language adapters are individually feature-gated; disabling a
      language does not pull in its frontend dependencies.
- [x] All selected languages use the common compiler path; execution uses
      ordinary `Runtime`, quotas, and explicit host capabilities.
- [x] The crate dependency graph remains acyclic; the current SDK dev-dependency
      from `crush-lang-rust` is removed or the integration is placed in a bridge
      crate with a documented reason.
- [x] SDK tests compile and run representative Rust corpus fixtures, including
      output and return behavior where supported.
- [x] Disabled feature builds do not pull in the Rust frontend.

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

## Notes

- Implementation is tracked in [PR #213](https://github.com/nixpt/crush-ast/pull/213).
- The integration fixes for CRUSH-xps-10 and CRUSH-xps-11 are included in that
  PR. Review and merge remain pending.

## Validation in PR #213

- Public API tests exercise language names, `.rs` selectors, disabled adapters,
  unknown selectors, and the preserved Crush return-value path.
- Rust integration tests now execute through the full semantic/optimizer/compiler
  path; the SDK/package CI command passes locally.
- A separate Rust host compiled and ran real `.rs` files with exact `rustc`
  output, and ran a `.crush` file that returned `Some(Int(42))`.
- Hosts explicitly register the SDK standard library for mapped methods;
  compilation does not grant host authority.

## v0.6.0 compatibility audit (before COSM transport integration)

Both review branches include the exact v0.6.0 tag. Internal dependency pins
remained 0.6.0; at this audit, VM, CASM and FFI source was unchanged. The public
`compile_source`/`compile_rust_source` APIs are additions after 0.6.0, not APIs
already available in the published SDK. Release notes now keep all SDK work
under Unreleased, preserving the tagged release history byte-for-byte.

Fresh locked source-API/corpus tests: 29 passed; disabled-adapter library test:
1 passed. The no-default normal dependency graph excludes Rust/tree-sitter.
Crates.io still has no crush-lang-rust entry as of this audit; that crate needs
a first publish before the next default-feature SDK release can publish.

## Current main integration

Merged main `3d512af` (COSM kind-table transport, PR #219) after the v0.6.0
comparison. This adds the upstream VM/CASM table plumbing to the branch, so the
older unchanged-core audit above describes its earlier head. The source API
inherits SDK CASM-to-VM table propagation; it does not enable Rust object syntax.
