# crush-ast — State (registered s298, 2026-06-16)

**Role:** the **portable Crush** language toolchain — polyglot source → **CAST** (AST IR) → **CASM** (bytecode) → **crush-vm** (CVM1 stack VM w/ quotas + capability gates). Frontends: Python (`rustpython-parser`) + Rust (`syn`) native; JS/Go/C/Bash/Zig/Wasm walkers are scaffolds. Ships `crushc`/`crush-run`/`crush-compile`/`crush-repl` + crush-pkg/crush-installer. Extracted from exosphere s277.

**Deps:** none external (self-contained cargo workspace). **Build:** `cargo check/test --workspace` standalone. 414 tests green (s298).

**Version:** workspace `0.2.0`, edition 2024, rust-version 1.95. **Remote:** `nixpt/crush-ast` (private), main `edcbe93` (pushed s298 — merged `polyglot`+`types`, left `rustpython` WIP).

**Doc hardening (CRUSHRUN-S2):** `crush-pkg`'s runner subsystem
documents the `ScriptRuntime` 4-variant cap (`manifest.rs:174`) as an
**intent-aware cap** — extending the enum is mechanical (2 places per
new runtime: add variant + `get_runtime_command` arm in `runners.rs`)
but NOT done speculatively. When a real language demand emerges
(Ruby / Go-script / Julia / Perl / R / ...), add the variant + binary
mapping; if activation is exploratory, gate the new runtime behind
`--strict` (the same opt-in gate CRUSHFMT-1 introduced for the
unknown-format run path — see PR #10). 78 `crush-pkg --bin` tests
unchanged (docs-only delta; no Rust code touched). Closes Gap 2 of
`TICKETS/CRUSHRUNNERS-1.md` (sister branch `agent/buffy/CRUSHRUNNERS-1`,
PR #7 — ticket file not yet merged into `2f2b2f5`).

**Live memory = `.dejavue/`** (boot with `dejavue context` — handoff/state/decisions/timeline). This STATE.md is a foreman-resume pointer; the dejavue is the source of truth. **Open work / roadmap → `TASKS.md`.**

**Foreman registration + cross-audit vs exosphere in-tree crush + known gaps:** see `workspace-meta/FOREMAN_THREADS.md` → "🌳 crush-ast".

**⚠️ Coordination:** captain/opencode-driven on [main] (a live `opencode -c` edits the shared working tree — do branch surgery via a throwaway `git worktree`, never checkout/stash the primary tree). Design archive: `docs/design/crushvm-rustpython.md`.
