# crush-ast — State (registered s298, 2026-06-16)

**Role:** the **portable Crush** language toolchain — polyglot source → **CAST** (AST IR) → **CASM** (bytecode) → **crush-vm** (CVM1 stack VM w/ quotas + capability gates). Frontends: Python (`rustpython-parser`) + Rust (`syn`) native; JS/Go/C/Bash/Zig/Wasm walkers are scaffolds. Ships `crushc`/`crush-run`/`crush-compile`/`crush-repl` + crush-pkg/crush-installer. Extracted from exosphere s277.

**Deps:** none external (self-contained cargo workspace). **Build:** `cargo check/test --workspace` standalone. 414 tests green (s298).

**Version:** workspace `0.2.0`, edition 2024, rust-version 1.95. **Remote:** `nixpt/crush-ast` (private), main `edcbe93` (pushed s298 — merged `polyglot`+`types`, left `rustpython` WIP).


### Test-helper-split (CRUSHPVMSPLIT-1, 2026-06-22)

Extract the dispatch logic from `crates/crush-vm/src/portable_vm.rs` into a
private submodule `crates/crush-vm/src/portable_vm/opcodes.rs` (sized S,
landed as `agent/buffy/CRUSHPVMSPLIT-1`, commit off `origin/main` `ba90cec`).
Two private methods moved: `execute_instruction(&mut self, opcode, next_ip)`
(663 lines) + `dispatch_cap(&mut self, cap, args)` (135 lines). Both became
`pub(super) fn` chokepoints taking `&mut super::PortableVm`. No pub-surface
change. `portable_vm.rs` shrinks 1235 -> 439 lines; `opcodes.rs` is 807 lines
(combined pre-amble + doc-comment + `use super::*; use crate::vm::{...}` + two
chokepoints + bodies). Step() now calls `opcodes::execute_instruction(self,
opcode, next_ip)?`. Test invariant preserved: `cargo test -p crush-vm` returns
**67 passed / 0 failed** before AND after (verified via per-binary diff of
`^test crush_vm` paths against `origin/main` — 0 differences). The original
sed-cascade failed 6 times; a single atomic Python script (60 lines,
/tmp/atomic_split.py) succeeded in one pass. The script's correctness hinges
on byte-exact depth-counter closure detection (signature accounted for via
`{` count increment) and a deterministic 5-step transform chain
(self.dispatch_cap->placeholder->free-fn form, Self::->super::PortableVm::,
self.foo->vm.foo, bare self->vm).

**Live memory = `.dejavue/`** (boot with `dejavue context` — handoff/state/decisions/timeline). This STATE.md is a foreman-resume pointer; the dejavue is the source of truth. **Open work / roadmap → `TASKS.md`.**

**Foreman registration + cross-audit vs exosphere in-tree crush + known gaps:** see `workspace-meta/FOREMAN_THREADS.md` → "🌳 crush-ast".

**⚠️ Coordination:** captain/opencode-driven on [main] (a live `opencode -c` edits the shared working tree — do branch surgery via a throwaway `git worktree`, never checkout/stash the primary tree). Design archive: `docs/design/crushvm-rustpython.md`.
