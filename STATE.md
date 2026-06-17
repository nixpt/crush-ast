# crush-ast — State (registered s298, 2026-06-16)

**Role:** the **portable Crush** language toolchain — polyglot source → **CAST** (AST IR) → **CASM** (bytecode) → **crush-vm** (CVM1 stack VM w/ quotas + capability gates). Frontends: Python (`rustpython-parser`) + Rust (`syn`) native; JS/Go/C/Bash/Zig/Wasm walkers are scaffolds. Ships `crushc`/`crush-run`/`crush-compile`/`crush-repl` + crush-pkg/crush-installer. Extracted from exosphere s277.

**Deps:** none external (self-contained cargo workspace). **Build:** `cargo check/test --workspace` standalone. 414 tests green (s298).

**Version:** workspace `0.2.0`, edition 2024, rust-version 1.95. **Remote:** `nixpt/crush-ast` (private), main `edcbe93` (pushed s298 — merged `polyglot`+`types`, left `rustpython` WIP).

**Live memory = `.dejavue/`** (boot with `dejavue context` — handoff/state/decisions/timeline). This STATE.md is a foreman-resume pointer; the dejavue is the source of truth. **Open work / roadmap → `TASKS.md`.**

**Foreman registration + cross-audit vs exosphere in-tree crush + known gaps:** see `workspace-meta/FOREMAN_THREADS.md` → "🌳 crush-ast".

**⚠️ Coordination:** captain/opencode-driven on [main] (a live `opencode -c` edits the shared working tree — do branch surgery via a throwaway `git worktree`, never checkout/stash the primary tree). Design archive: `docs/design/crushvm-rustpython.md`.
