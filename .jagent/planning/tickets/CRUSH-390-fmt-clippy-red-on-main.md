# CRUSH-390 — `cargo fmt --check` and `clippy -D warnings` are red on main

| Field | Value |
|-------|-------|
| **ID** | CRUSH-390 |
| **Priority** | P1 |
| **Status** | Done |
| **Phase** | CI hygiene |
| **Assignee** | opencode |
| **Dependencies** | none |
| **Estimated effort** | M |

## Problem

`cargo fmt --all -- --check` reported ~3500 diff hunks across casm, crush-aot,
crush-aotc, crush-lang-go and many other crates, so any brief or contributor
running the canonical format/lint gates fails on step one.
`cargo clippy -p crush-lang-sdk --all-targets -- -D warnings` failed in
dependency `crush-ffi` (`should_implement_trait`: `FfiString::from_str` can be
confused for `std::str::FromStr::from_str`) — and kept failing behind it: the
whole `crush-lang-sdk` dependency tree had accumulated ~150 clippy violations
under `-D warnings` (clippy lint args are not part of cargo's fingerprint, so
units cached from warn-level runs silently masked most of them).

## Evidence

Measured at `main` `d0b7cc03` (v0.11.2), 2026-11-10.

## Success criteria

- [x] `cargo fmt --all -- --check` exits 0
- [x] `cargo clippy -p crush-lang-sdk --all-targets -- -D warnings` exits 0
- [x] The CI clippy gate (`cargo clippy --workspace --exclude crush-python --exclude crush-lang-custom --exclude crush-gui -- -D clippy::never_loop`) stays green

## Technical approach

- Mechanical: `cargo fmt --all` (311 files; no rustfmt.toml — default style at
  the workspace edition 2024). Whitespace/reflow only.
- `crush-ffi`: `FfiString::from_str` borrows its input ("caller must keep source
  alive"), so implementing `FromStr` would be wrong. Documented
  `#[allow(clippy::should_implement_trait)]` with the reasoning, instead of an
  API-breaking rename in a published crate.
- `crush-vm`, `crush-frontend`, walkers, `crush-net`, `crush-index`,
  `crush-lang-sdk`: fixed every violation in place (let-chains for collapsible
  ifs, `matches!`/`is_some_and`/`contains`/`sort_by_key` idiom rewrites, dead
  code deleted, doc-comment indentation, `Default` impls, renamed the
  unpublished `crush-net` `NetState::next` → `alloc_conn`).
- `crush-walker-core`: feature-gated `Context`/`ast` imports — the crate is
  built twice by the SDK (with and without `tree-sitter`), so un-gated imports
  are unused in the no-tree-sitter copy.
- Judgment calls allowed with rationale (not silenced silently):
  `NumericValue::add/sub/mul/div` (return `RuntimeValue`, not `Self` — the ops
  traits' contract cannot be met), `execute_one`'s 9-arg VM dispatch signature,
  `Commands` in the `crush-run` CLI (clap destructuring churn).
- `render.rs` had a **duplicated, unreachable** `Statement::Assign` arm
  (copy-paste). Removed dead arm; behavior unchanged. Worth a follow-up look at
  whether the second arm was meant to render something different.

## Resolution

This commit. All three gates verified from a clean, dedicated
`CARGO_TARGET_DIR` (the shared one cross-contaminates cached diagnostics between
worktrees — see the dejavue note). Tests for every touched crate green. If a
follow-up wants `-D warnings` enforced workspace-wide in CI, that is a CI-policy
change (separate ticket) — this ticket only makes the commands from the issue
pass.
