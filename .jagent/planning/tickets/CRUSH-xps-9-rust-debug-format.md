# CRUSH-xps-9 — Match or reject Rust `{:?}` formatting

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-9 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M2 / Rust parity |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-xps-3, CRUSH-xps-5 |
| **Estimated effort** | M |

## Problem

The Rust walker supports debug-format placeholders, but Crush's runtime values
do not necessarily format like Rust's `Debug` implementation. Accepting a
placeholder with different output silently violates the Rust parity contract.

## Success criteria

- [ ] Every supported `{:?}` value prints the same representation as the
      corresponding `rustc` fixture, or the placeholder/type is rejected with
      a useful diagnostic.
- [ ] Fixtures cover strings, booleans, numbers, vectors, and nested supported
      values.
- [ ] The known-silent entry is removed only after VM exact and AOT exact-or-
      loud checks pass.

## Technical approach

- Specify the supported Rust `Debug` subset rather than mapping all values to
  Crush's generic formatter.
- Compare output with `rustc` and keep unsupported format/value pairs loud.

## Non-goals

- Implementing all of `std::fmt::Debug` or Rust formatting traits.
- Supporting width/precision formats such as `{:.2}` or `{:>5}`.
