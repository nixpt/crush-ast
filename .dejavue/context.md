---
name: crush-ast
purpose: Standalone Crush language toolchain — CAST IR, tree-sitter grammar, polyglot walkers, compiler frontend, VM runtime (CVM1 / FastVM / JIT / AOT), debugger, GUI, package manager, installer. Published to crates.io as crush-lang-sdk and friends.
dcp: DCP/1.0
---

# Context

Refreshed 2026-10-31 against `main` (workspace 0.8.0, 44 members). History of why things are the way
they are is in `decisions.md` / `timeline.jsonl`; this file is the current picture.

## Operating Rules

- All internal crate deps use `workspace = true`; workspace deps carry both `path` + `version`
  (a real dep without `version` can never publish).
- Dependency DAG is acyclic: `crush-frontend` → `crush-cast` → `casm` → `crush-errors`. No back-edges.
- The only path dependency outside the repo is `../buckets` (`crush-pkg`, `crush-vm`; published as
  `crush-buckets`). Everything else comes from crates.io.
- One capability, one implementation, every engine calls it (`io_print.rs`, `io_read.rs`).
- Capabilities are opt-in per run, never ambient; a program that needs an ungranted one is refused
  before it runs (`crush_lang_sdk::effects::missing_grants`), naming the flag.
- New walker: implement `LanguageAdapter` (`crush-walker-core`); a construct it can't lower exactly is
  refused loudly with its position, never approximated.
- Ticket IDs come from `cargo run -p xtask --bin ticket-id -- next`; CI rejects duplicates.

## Architecture

1. **Core IR**: `crush-errors` → `crush-cast` (CAST) → `casm` (CASM); `crush-cosm` (object model);
   `crush-caison` (CAISON data, `caison.parse`); `crush-diagnostics` (the one NDJSON diagnostic schema).
2. **Grammar**: `tree-sitter-crush` — parses 49/51 `examples/crush/*.crush` cleanly, checked in CI
   (`tests/grammar_examples.rs`).
3. **Walkers** (`crush-walker-core::LanguageAdapter`; source → CAST): Python (rustpython), Rust (syn),
   JS/TS (swc + boa), Bash (brush) and Zsh (zshrs) over one shared shell lowering, C/C++, Go, Zig,
   Dart (tree-sitter), Wasm (wasmparser); Java, Nepali and custom are thin.
4. **Compiler**: `crush-frontend` (parse → semantics → optimize → CASM).
5. **Engines**: CVM1 `PortableVm` + scheduler (`crush-vm`), FastVM, Cranelift JIT (`crush-jit`), AOT C/Rust
   (`crush-aot`, `crush-aotc`), PTX (`crush-ptx`), browser (`crush-web`, outside the workspace).
6. **Host & tools**: `crush-lang-sdk` (HostCaps, stdlib, polyglot `@python`/`@javascript`/`@bash`,
   `net.*`, binaries `crush`, `crushc`, `crush-run`, `crush-repl`, `crush-compile`, `crush-diff`,
   `crush-debug`), `crush-debugger`, `crush-net` (`tcp.*`), `crush-gui` (`ui.*` on arniko/bliss),
   `crush-pkg`, `crush-installer`, `crush-lint`, `crush-index`.

## Runtime values (crush-vm `Value`)

`Null`, `Bool`, `Int` (i64), `Float`, `Str`, `Array`, `Tuple`, `Set`, `Map` (insertion-ordered),
`Error`, `Bytes`, `Func` (closures; captured variables live in `Cell`s), `Record` (struct instances, COSM).

## Key Decisions (see decisions.md for the reasoning)

- Parser-only polyglot: walkers translate source to CAST; `@lang { }` blocks run in host runtimes
  (python, node, bash) behind a `polyglot.<lang>` grant — no embedded interpreters.
- One VM bytecode (CVM1); FastVM/JIT/AOT must agree with it (`backend_agreement`, `examples_parity`).
- Pre-1.0 releases: each tag gets a GitHub pre-release with prebuilt artifacts; a breaking change bumps
  minor while the version is 0.x.

## External dependents (published crates)

- `crush-notebook` (kernel 0.2.0) → `crush-lang-sdk`, `crush-frontend`, `crush-vm`, `crush-jit`
- `crush-lsp` 0.1.0 → `crush-frontend`; `crush-squeeze` 0.1.0 → `crush-pkg`
- `arniko-crush` 0.3.2 → `crush-lang-sdk`; `polydex` → `tree-sitter-crush`
- `crush-visuals` → `crush-debugger`, `crush-vm`, `crush-frontend`, `crush-cast`
- The language guide (docs.crushlang.org) and crushlang.org ship `crush-web`'s pkg; double-crush uses
  a pinned crush-ast build as its oracle.
