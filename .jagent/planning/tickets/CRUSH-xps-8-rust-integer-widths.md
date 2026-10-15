# CRUSH-xps-8 — Match Rust integer widths and casts in the Rust walker

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-8 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M2 / Rust parity |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-xps-3, CRUSH-xps-5 |
| **Estimated effort** | M |

## Problem

Crush currently represents integers as signed 64-bit values, but the Rust
walker accepts narrower integer annotations and `as` casts without preserving
their truncation/extension behavior. For example, `3_000_000_000_i64 as i32`
must produce `-1294967296`, not `3000000000`.

## Success criteria

- [ ] Rust integer-width conversions that are accepted match `rustc` exactly;
      unsupported widths or casts fail loudly.
- [ ] Overflow and `wrapping_*` operations match Rust or fail loudly; no
      accepted expression silently returns a widened/non-wrapped result.
- [ ] The VM parity corpus and AOT exact-or-loud harness cover casts, signed
      boundaries, and wrapping behavior.

## Technical approach

- Decide how narrow signed and unsigned values are represented or converted in
  the Crush runtime before accepting annotated Rust expressions.
- Use `rustc` as the oracle for cast, overflow, and wrapping fixtures.
- Keep unsupported `wrapping_add` in the known-loud list until its behavior is
  exact on VM and AOT.

## Non-goals

- Adding arbitrary-precision integers to Crush.
- Claiming full Rust integer type checking.
