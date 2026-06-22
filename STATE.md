# crush-ast — State (registered s298, 2026-06-16)

**Role:** the **portable Crush** language toolchain — polyglot source → **CAST** (AST IR) → **CASM** (bytecode) → **crush-vm** (CVM1 stack VM w/ quotas + capability gates). Frontends: Python (`rustpython-parser`) + Rust (`syn`) native; JS/Go/C/Bash/Zig/Wasm walkers are scaffolds. Ships `crushc`/`crush-run`/`crush-compile`/`crush-repl` + crush-pkg/crush-installer. Extracted from exosphere s277.

**Deps:** none external (self-contained cargo workspace). **Build:** `cargo check/test --workspace` standalone. 414 tests green (s298).

**Version:** workspace `0.2.0`, edition 2024, rust-version 1.95. **Remote:** `nixpt/crush-ast` (private), main `edcbe93` (pushed s298 — merged `polyglot`+`types`, left `rustpython` WIP).

**Test hardening (CRUSHCN-1):** `crush-pkg`'s runner subsystem drops
the `ContainerRunner` stub (`runners.rs:113-129` deleted; 2 dispatch
arms in `get_runner` + `get_runner_for_payload` replaced with
comment-only markers; 1 stub-pin test `test_get_runner_container_stub`
removed) and rejects `language = "container"` at parse-time via new
`Manifest::validate_language` + bail hook in `Manifest::from_str`
(`manifest.rs`). Deletion path selected per the user's "if no
container story is on the roadmap" conditional — zero refs to
container / docker / oci / wasi in STATE.md / TASKS.md / builder.rs /
main.rs / Cargo.toml. Test invariant: 78 `crush-pkg --bin` tests
→ 79 (lost `test_get_runner_container_stub`, gained
`language_container_string_rejected_at_parse` +
`language_unknown_string_still_falls_through_to_auto`). Closes Gap 1
of the runner-subsystem catalogue at `TICKETS/CRUSHRUNNERS-1.md`
(CRUSHRUNNERS-1 PR #7).

**Live memory = `.dejavue/`** (boot with `dejavue context` — handoff/state/decisions/timeline). This STATE.md is a foreman-resume pointer; the dejavue is the source of truth. **Open work / roadmap → `TASKS.md`.**

**Foreman registration + cross-audit vs exosphere in-tree crush + known gaps:** see `workspace-meta/FOREMAN_THREADS.md` → "🌳 crush-ast".

**⚠️ Coordination:** captain/opencode-driven on [main] (a live `opencode -c` edits the shared working tree — do branch surgery via a throwaway `git worktree`, never checkout/stash the primary tree). Design archive: `docs/design/crushvm-rustpython.md`.
