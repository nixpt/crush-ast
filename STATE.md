# crush-ast — State (registered s298, 2026-06-16)

**Role:** the **portable Crush** language toolchain — polyglot source → **CAST** (AST IR) → **CASM** (bytecode) → **crush-vm** (CVM1 stack VM w/ quotas + capability gates). Frontends: Python (`rustpython-parser`) + Rust (`syn`) native; JS/Go/C/Bash/Zig/Wasm walkers are scaffolds. Ships `crushc`/`crush-run`/`crush-compile`/`crush-repl` + crush-pkg/crush-installer. Extracted from exosphere s277.

**Deps:** none external (self-contained cargo workspace). **Build:** `cargo check/test --workspace` standalone. 414 tests green (s298).

**Version:** workspace `0.2.0`, edition 2024, rust-version 1.95. **Remote:** `nixpt/crush-ast` (private), main `edcbe93` (pushed s298 — merged `polyglot`+`types`, left `rustpython` WIP).


### Test-helper-split (CRUSHPVMSPLIT-1a, 2026-06-22)

First of a two-extract sequence: extract ONLY `dispatch_cap` (135 lines)
from `crates/crush-vm/src/portable_vm.rs` into a private submodule
`crates/crush-vm/src/portable_vm/opcodes.rs` (`execute_instruction`
stays in `portable_vm.rs` for now; CRUSHPVMSPLIT-1b will move it later).
Smaller blast radius than CRUSHPVMSPLIT-1 (PR #11). Sized XS.

`dispatch_cap(&mut self, cap: &str, args: Vec<Value>) -> Result<Option<Value>, VmError>`
became a `pub(super) fn dispatch_cap(vm: &mut super::PortableVm, ...)`
chokepoint taking the parent as `&mut`. The single call site inside
`execute_instruction` (`self.dispatch_cap(cap, args)` at line ~531 pre-extract)
was rewritten to `opcodes::dispatch_cap(self, cap, args)`. `mod opcodes;`
declared just before `pub struct PortableVm` in `portable_vm.rs`. Pub
surface unchanged (`lib.rs:16` untouched). `cargo test -p crush-vm`
preserves the pre-extraction baseline (verified by per-binary test-name
diff against `origin/main`; expect 0 differences).

**Live memory = `.dejavue/`** (boot with `dejavue context` — handoff/state/decisions/timeline). This STATE.md is a foreman-resume pointer; the dejavue is the source of truth. **Open work / roadmap → `TASKS.md`.**

**Foreman registration + cross-audit vs exosphere in-tree crush + known gaps:** see `workspace-meta/FOREMAN_THREADS.md` → "🌳 crush-ast".

**⚠️ Coordination:** captain/opencode-driven on [main] (a live `opencode -c` edits the shared working tree — do branch surgery via a throwaway `git worktree`, never checkout/stash the primary tree). Design archive: `docs/design/crushvm-rustpython.md`.
