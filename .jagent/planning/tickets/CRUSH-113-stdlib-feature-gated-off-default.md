# CRUSH-113 — default build ships no stdlib capabilities; `--stdlib` silently no-ops

| Field | Value |
|-------|-------|
| **ID** | CRUSH-113 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none (see CRUSH-108 for the stdlib source-reconciliation ticket) |
| **Estimated effort** | S |

## Problem

The `stdlib` cargo feature (`crush-lang-sdk/Cargo.toml`, `stdlib =
["dep:regex"]`) is **not in `default`**, so the default `crushc`/`crush-run`
build registers none of the `crush-lang-sdk/src/stdlib.rs` host caps
(`conv.*`, `collections.*`, `json.*`, `path.*`, `regex.*`, and the `math.*`
caps). Running `crush-run --stdlib` only prints
`warning: --stdlib requires the 'stdlib' feature (not enabled in this build)`
(`bin/crush-run.rs:335`) and does nothing.

Consequence: the default toolchain has **no int↔string conversion, no
`chr`/`ord`, no `parse_int`, no collections/regex/json helpers** — primitives
a real program expects. The `awesome-crush` collection is a direct artifact:
every game hand-rolled an LCG RNG (no RNG primitive) and the interpreters
hand-rolled integer parsing and a printable-ASCII character table.

## Impact

Users following the README's `cargo build --release -p crush-lang-sdk --bin
crushc --bin crush-run` get a toolchain missing fundamental
conversion/collection/regex/math primitives, and `--stdlib` looks like a
supported flag while doing nothing.

## Reproduction

```bash
crush-run caps   # lists only the ~13 portable caps; no conv.*, collections.*, math.*, json.*
printf 'fn main() { io.print(conv.to_str(42)); return 0; }\n' > /tmp/c.crush
crushc /tmp/c.crush -o /tmp/c.cvm1 && crush-run run /tmp/c.cvm1 --stdlib --cap io.print
# warning: --stdlib requires the 'stdlib' feature (not enabled in this build)
# [runtime] unknown capability: conv.to_str
```

## Success criteria

- [ ] Decide whether `stdlib` should be default-on; if it stays opt-in, `--stdlib` must **error** (not warn) when the feature is absent.
- [ ] `conv.to_int`/`conv.to_str`/`conv.parse_int` (and the rest of `stdlib.rs`) are available in the default build, or gated behind a loud, documented error.
- [ ] `crush-run --help` / the README state the feature requirement.

## Technical approach

- Either add `"stdlib"` to `default` in `crush-lang-sdk/Cargo.toml` (the `regex` dep is small), or make the `#[cfg(not(feature = "stdlib"))]` arm of `crush-run.rs` a hard error.
- Cross-check with CRUSH-108: that ticket already found `stdlib.rs` implements and wires 59 stdcaps that CRUSH-56/88..97 don't reference — so this is an availability/flag question, not missing implementations.

## Files to modify

- `crates/crush-lang-sdk/Cargo.toml` — feature default.
- `crates/crush-lang-sdk/src/bin/crush-run.rs` — warn → error, or enable by default.
- `README.md` / `crates/crush-lang-sdk/README.md` — document the flag + feature.

## Non-goals

- Implementing new conversion/collection capabilities (they already exist in `stdlib.rs`).
- The broader M9 stdlib restoration (CRUSH-56/88..97).
