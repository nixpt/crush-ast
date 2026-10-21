# CRUSH-286 — Unique ticket IDs: a CI check and an allocator

| Field | Value |
|-------|-------|
| **ID** | CRUSH-286 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | M0 |
| **Assignee** | cursor (dispatched by kai) |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

Ticket files live in `.jagent/planning/tickets/CRUSH-<N>-<slug>.md`. Agents and
two boxes picked numbers in parallel and nothing checked them: CRUSH-275, 280,
281, 283 and 284 were each claimed twice in two days (one ticket was renumbered
four times), and `main` already carried older duplicates (CRUSH-266, CRUSH-271,
CRUSH-31, CRUSH-33, CRUSH-34 each have two files).

## Success criteria

- [x] A check runs in CI with no workflow change: `cargo test --tests -p xtask`
      (already a CI job) fails when two files under `.jagent/planning/tickets/`
      or `.jagent/planning/archive/` share an ID, listing each clash with both
      file names. IDs are `CRUSH-<digits>[<lowercase letter>]` plus namespaced
      `CRUSH-zorro-<digits>` / `CRUSH-xps-<digits>` style prefixes.
- [x] Existing clashes are grandfathered explicitly in
      `xtask/ticket-id-allowlist.txt` (one ID per line, with a reason); old
      tickets are not renumbered. Every *new* duplicate fails.
- [x] `cargo run -p xtask --bin ticket-id -- next` prints the next free number,
      considering `origin/main`, every `refs/remotes/origin/*` branch
      (`git ls-tree`), and — when `gh` works — open PR titles; if `gh` is
      unavailable it says so on stderr and falls back to git only.
      `ticket-id -- check` runs the same check locally.
- [x] Docs: a "Ticket numbers" section in `CONTRIBUTING.md`.

## Technical approach

- Shared implementation in `xtask/src/ticket_ids.rs` (parsing, duplicate
  detection, allow-list, repo scan, next-free computation) used by both the
  test and the `ticket-id` binary, so they cannot drift.
- Test in `xtask/tests/ticket_ids.rs`; binary in `xtask/src/ticket_id.rs`
  (stdout carries only the allocated ID; warnings go to stderr).

## Files to modify

- `xtask/src/ticket_ids.rs` — shared check/allocator logic (new)
- `xtask/src/ticket_id.rs`, `xtask/Cargo.toml`, `xtask/src/lib.rs` — `ticket-id` binary wiring (new)
- `xtask/tests/ticket_ids.rs` — CI gate + unit tests (new)
- `xtask/ticket-id-allowlist.txt` — grandfathered duplicates (new)
- `CONTRIBUTING.md` — "Ticket numbers" section

## Non-goals

- Renumbering or deleting the grandfathered duplicate tickets.
- Changing any CI workflow file.
