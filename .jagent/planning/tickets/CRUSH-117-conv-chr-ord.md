# CRUSH-117 — Add `conv.chr`/`conv.ord`: character ↔ codepoint primitives

| Field | Value |
|-------|-------|
| **ID** | CRUSH-117 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

Crush has no `chr()`/`ord()` — confirmed by grep across `stdlib.rs`: zero
hits. `examples/crush/brainfuck.crush` had to hand-build a 95-character
string of printable ASCII (32..126) and index it by `value - 32` specifically
because there is no primitive to turn a byte value into a character (its own
header comment documents this exact workaround). Any future text-processing
program that needs to go from a computed integer to a character — or a
character to its codepoint, for parsing — hits the same wall.

## Approach

- Add `conv.chr(codepoint)` → single-character string and `conv.ord(s)` → int
  (codepoint of `s`'s first character; error on empty string or on a string
  with more than one character) to `stdlib.rs`, alongside the existing
  `Conv*Cap` family (`ConvToIntCap`, `ConvToStrCap`, etc.).
- **Decide on-by-default vs. gated before implementing**: `stdlib.rs`'s caps
  currently sit behind the `stdlib` cargo feature, which CRUSH-113 found is
  **off by default** (`--stdlib` silently no-ops without it). `chr`/`ord` are
  fundamental enough — on par with `io.print`, not an optional extra — that
  they may belong in `crush-vm/src/caps.rs`'s always-on portable registry
  instead of behind the same gate CRUSH-113 flags as broken. Cross-reference
  CRUSH-113's resolution before picking a home; don't silently inherit its
  "gated and broken" default.

## Definition of done

- [ ] `conv.chr`/`conv.ord` implemented, in whichever registry CRUSH-113's
      resolution points to
- [ ] Clear error behavior on out-of-range codepoints / non-single-char
      `ord()` input
- [ ] `@covers` test through the real pipeline
- [ ] Nice-to-have, not blocking: port `brainfuck.crush`'s hand-built ASCII
      lookup table to use `conv.chr`, as a real proof of adoption

## Files to modify

- `crates/crush-lang-sdk/src/stdlib.rs`
- `crates/crush-vm/src/caps.rs` — if promoted to a portable (always-on) cap

## Gates

Loosely related to CRUSH-113 (where the cap should live) — read that
ticket's resolution first, not strictly blocked by it.
