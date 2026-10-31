# State

Updated: 2026-10-31 (main `faeb86e8`, workspace 0.8.0). Earlier snapshots are in git history.

## Released

- **v0.8.0** (2026-10-30): 27 crates on crates.io; `cargo install crush-lang-sdk` gives `crush`,
  `crushc`, `crush-run`, `crush-repl`, `crush-compile`, `crush-diff`. 15 workspace crates have never
  been published (10 walkers, `crush-aot`, `crush-net`, `crush-tui`, `crush-vm-capi`, `crush-walker`),
  plus `crush-gui`.
- **0.9.0** is being cut from `main` (2026-10-31): the first release with a GitHub pre-release and
  prebuilt artifacts.

## On main since v0.8.0 (headline)

- Engines agree on more: FastVM/JIT print heap values (#319), canonical equality (#317), division edge
  cases fail loudly (#318), JIT frame stride (#316), deterministic CASM layout (#324).
- Language: string slices and `[::step]` (#347), closures work end to end.
- Diagnostics: one JSON schema; `crushc`/`crush-run` emit identical records; semantic errors carry
  line/col (#338). `crush-lint` reports real parser/semantic findings (#336).
- Tools: `crush debug` / `crush-debug` (#341), `crush gui` / `ui.*` (#344), `tcp.*` sockets behind
  `--tcp`/`--tcp-listen` (#343), `net.*` on by default and renamed `http.*` refused by name (#340).
- Walkers: bash/zsh pipelines, redirections, builtins, globs (#322, #327, #334, #345); loud failures
  instead of silent drops in the Go/Zig/Dart/Java/Wasm walkers (#323, #335).
- tree-sitter grammar tracks the frontend; CI checks the corpus (#342).
- Release engineering: publish keeps going past a refused crate (#320); unique ticket IDs in CI (#291);
  pre-release + artifacts per tag, cheaper PR CI (#346); no accidental 1.0 (#348, pending).

## Known open (see the 1.0 criteria doc for the full list)

- #258 (`@capabilities` ignored by crushc), #261 (ungranted AI op → null), CRUSH-110 (`import` loads
  nothing), CRUSH-219 (runtime errors not catchable on every engine), CRUSH-211 (conformance not in CI).
- Walkers: C (#329–#331), shell arithmetic (#332), undocumented refusals (#333).
- `crush-vm` still path-depends on `../buckets` (nixpt/buckets#7).
- `docs/planning/RELEASE-1.0-CRITERIA.md` (PR #281): 14 ✅ / 26 🟡 / 27 ❌ as of 2026-10-31.
