# CRUSH-203 — Zig/Wasm/Dart walkers produce empty or null programs and exit 0; Java panics; Nepali claim is false

| Field | Value |
|-------|-------|
| **ID** | CRUSH-203 |
| **Priority** | P1 |
| **Status** | Done |
| **Phase** | M6 |
| **Assignee** | nixp |
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

- [x] Any unhandled node is a walker error naming the node kind and source location.
- [x] Stub walkers fail with "not implemented" (non-zero exit) instead of an empty program.
- [x] `crush-walk-run` reads `.wasm` as bytes.
- [x] Nepali docs corrected (or keywords added).

## Resolution (2026-10-30, `agent/nixp/CRUSH-203`)

- **Zig** (`crush-lang-zig`): the walker's node names did not match
  tree-sitter-zig (`IntegerLiteral` vs the real `integer`/`float`/`string`/
  `boolean`; `BinaryExpr` vs `binary_expression`; `CallExpr` vs
  `call_expression`), so every literal silently became `null` and every program
  printed nothing. The names are fixed, the argument list (direct children, no
  `arguments` field), `labeled_statement`/`block_expression`/`expression_statement`
  wrappers and `.{}` initializers are handled, and the two `null`/empty
  fallbacks are now loud errors naming the node kind and position.
- **Dart** (`crush-lang-dart`): the walker discarded every node and returned an
  empty program; it now fails with "not implemented".
- **Java** (`crush-lang-java`): `language()`/`parse()` no longer panic with a
  bare `unreachable!` (a clear message instead), and `walk()` — which returned
  `Program::default()` — fails with "not implemented".
- **Wasm** (`crush-lang-wasm`): non-`call` operators and imported calls
  (including `fd_write`, which used to become an arg-less `io.print`) are now
  loud errors instead of silent drops.
- **`crush-walk-run`** reads a `.wasm` file as bytes (it used to fail with
  "stream did not contain valid UTF-8").
- **Nepali** (`crush-lang-nepali`): the false "natively parses Nepali keywords"
  comment is corrected — `.np`/`.nepali` use the standard Crush parser.

### Evidence

- Live: `crush-walk-run t.zig` (a `const x = 42; std.debug.print(x)`) prints
  `42` (before: nothing); `crush-walk-run t.wasm` reads the binary module (was
  a UTF-8 error).
- Unit tests per crate: the Zig literal/wrapper lowering, the Dart/Java/Wasm
  "not implemented"/loud paths (verified RED: reverting the Zig literal name
  fails `test_variable_declaration`).
- `cargo test --workspace --lib --tests`: 2572 passed, 0 failed.

## Follow-up

The Java walker stays a skeleton until `tree-sitter-java` is added (CRUSH-37);
`crush-lang-java` is still registered/advertised, but any use now fails loudly.
