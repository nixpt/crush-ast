# CRUSH-203 — Zig/Wasm/Dart walkers produce empty or null programs and exit 0; Java panics; Nepali claim is false

| Field | Value |
|-------|-------|
| **ID** | CRUSH-203 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M6 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

- **Zig:** `std.debug.print` and most nodes become `NullLiteral`
  (`crush-lang-zig/src/lib.rs:~378`) — every program prints nothing, exit 0.
- **Wasm:** only `call` is lowered, everything else `_ => {}` (`crush-lang-wasm/src/lib.rs:~107`);
  `fd_write` becomes `io.print` with no args. `crush-walk-run` can't read binary
  `.wasm` (`stream did not contain valid UTF-8`).
- **Dart:** the walker loop discards every node (`crush-lang-dart/src/lib.rs:~28-31`).
- **Java:** `crush-lang-java` panics on any input (`unreachable!: JavaWalker::language() is a stub`,
  `lib.rs:~73`) while `--help` advertises usage. Tracked as CRUSH-37.
- **Nepali:** `lib.rs` says "lexer natively parses Nepali keywords"; no Nepali keyword
  exists — it's Crush syntax under `.np`.
- A program with no entry `main` compiles and runs "successfully" with no output.

## Success criteria

- [ ] Any unhandled node is a walker error naming the node kind and source location.
- [ ] Stub walkers fail with "not implemented" (non-zero exit) instead of an empty program.
- [ ] `crush-walk-run` reads `.wasm` as bytes.
- [ ] Nepali docs corrected (or keywords added).
