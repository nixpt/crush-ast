# crush-ast — State (registered s298, 2026-06-16)

### CRUSHPVMDOCS-1 — Doc-only resolution to portable_vm.rs size concern (closed 2026-06-22)

**Resolution chosen: skip the extraction entirely; document the future-split intent in a module-level doc-comment at the top of `portable_vm.rs`.**

Rationale: seven attempts to extract the dispatch logic from `portable_vm.rs` into `portable_vm/opcodes.rs` produced recurring brittle-transform risk that exceeds the maintenance cost of leaving the dispatch inline. The doc-comment (top of `portable_vm.rs`) now records the full structure of the module, the rationale for keeping it cohesive, and a three-step recipe for any future re-attempt of CRUSHPVMSPLIT-1b. The two open PRs (#11 for the combined extraction, #12 for the smaller-scope `dispatch_cap`-only extraction) remain on disk as prior art but are marked defer-not-cancel.

**Role:** the **portable Crush** language toolchain — polyglot source → **CAST** (AST IR) → **CASM** (bytecode) → **crush-vm** (CVM1 stack VM w/ quotas + capability gates). Frontends: Python (`rustpython-parser`) + Rust (`syn`) native; JS/Go/C/Bash/Zig/Wasm walkers are scaffolds. Ships `crushc`/`crush-run`/`crush-compile`/`crush-repl` + crush-pkg/crush-installer. Extracted from exosphere s277.

**Deps:** none external (self-contained cargo workspace). **Build:** `cargo check/test --workspace` standalone. 414 tests green (s298).

**Version:** workspace `0.2.0`, edition 2024, rust-version 1.95. **Remote:** `nixpt/crush-ast` (private), main `edcbe93` (pushed s298 — merged `polyglot`+`types`, left `rustpython` WIP).

**Test hardening (CRUSHVM-1):** `crush-vm`'s `portable_vm` now carries a
`test_portable_*` parity suite in `portable_vm.rs::mod tests` mirroring
canonical `tests.rs` — 11 tests pin behaviour for `PUSH_BOOL`,
`NEW_OBJ` / `SET_FIELD` / `GET_FIELD`, `ENTER_TRY` / `EXIT_TRY` /
`THROW`, `ARR_PUSH` / `ARR_POP`, and the `Value::Map` type-name.
80 `crush-vm --lib` tests green (was 69). Implementation behaviour
between the two VMs is now locked by the suite, not by inspection.
`EXEC_LANG` runtime parity is tracked separately by
`TICKETS/CRUSHVM-2-EXEC-LANG-POP-NAMED.md`.

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
