# crush-ast — State (registered s298, 2026-06-16)

**Role:** the **portable Crush** language toolchain — polyglot source → **CAST** (AST IR) → **CASM** (bytecode) → **crush-vm** (CVM1 stack VM w/ quotas + capability gates). Frontends: Python (`rustpython-parser`) + Rust (`syn`) native; JS/Go/C/Bash/Zig/Wasm walkers are scaffolds. Ships `crushc`/`crush-run`/`crush-compile`/`crush-repl` + crush-pkg/crush-installer. Extracted from exosphere s277.

**Deps:** none external (self-contained cargo workspace). **Build:** `cargo check/test --workspace` standalone. 414 tests green (s298).

**Version:** workspace `0.2.0`, edition 2024, rust-version 1.95. **Remote:** `nixpt/crush-ast` (private), main `edcbe93` (pushed s298 — merged `polyglot`+`types`, left `rustpython` WIP).

**Test hardening (CRUSHPKG-1):** `crush-pkg`'s `capsule.toml` lint
subsystem now carries three load-bearing pin tests in
`crates/crush-pkg/src/main.rs::mod tests`:

- `handle_lint_with_byte_exact_three_rule_fedpath` — byte-exact
  NDJSON snapshot across the 3 current dead-code rule families
  (`ObsoleteKey` + `PlaceholderValue` + `UnreferencedDependency`)
  in TOML-line order; downstream editor + CI consumers depend
  on the wire shape byte-for-byte, so any drift trips a single
  `assert_eq!` with a localised diff.
- `handle_lint_with_referenced_dep_suppresses_finding_end_to_end`
  — full entry-aware wiring pin (`Manifest::from_str` →
  `parent().join(&entry)` → `scan_entry_file_references` →
  `lint_capsule_toml_with_entry`): a `[[dependencies]]` row whose
  name appears in the on-disk entry file (post `#`-strip + post
  URL-fragment fix) MUST NOT surface a dead-code finding.
- `scan_entry_file_references` URL-fragment fix in
  `crates/crush-pkg/src/builder.rs` — a string-literal `#`
  followed by URL-fragment characters no longer truncates the
  preceding identifier; HTTP/HTTPS-style URLs survive the
  reference set. Rationale at `builder.rs:998-1007`.

78 `crush-pkg --bin` tests green (binary-only crate:
`cargo test -p crush-pkg --lib` runs zero, since there is no
`lib.rs`; the test surface lives in `main.rs::mod tests`).
The NDJSON wire shape `crush-pkg` emits is locked at multi-rule
fedpath + entry-aware cross-ref granularity — no longer by
inspection. Implementation shipped in commit `2f2b2f5`.

**Live memory = `.dejavue/`** (boot with `dejavue context` — handoff/state/decisions/timeline). This STATE.md is a foreman-resume pointer; the dejavue is the source of truth. **Open work / roadmap → `TASKS.md`.**

**Foreman registration + cross-audit vs exosphere in-tree crush + known gaps:** see `workspace-meta/FOREMAN_THREADS.md` → "🌳 crush-ast".

**⚠️ Coordination:** captain/opencode-driven on [main] (a live `opencode -c` edits the shared working tree — do branch surgery via a throwaway `git worktree`, never checkout/stash the primary tree). Design archive: `docs/design/crushvm-rustpython.md`.
