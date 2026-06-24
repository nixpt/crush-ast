# crush-ast — TASKS / Roadmap

Actionable task tracker for the portable-Crush toolchain. Grouped by priority,
not by date. Check items off as they land; move shipped items to the **Done**
log at the bottom. The live architectural narrative (decisions, handoff, state)
lives in **`.dejavue/`** — boot with `dejavue context`. Foreman cross-audit +
exosphere comparison: `workspace-meta/FOREMAN_THREADS.md` → "🌳 crush-ast".

> Status baseline: workspace v0.2.0, `cargo test --workspace` = **414 green**
> (rustc 1.96). main `edcbe93`.

---

## 🔴 P0 — correctness / build health

- [x] **Fix `--all-features` build** — `db` and `stdlib` features fail to compile
  with `Value::Bool` type errors (fallout from the s298 VM type expansion that
  added `Bool`/`Map`/`Error`/`Bytes`). Default build + `cargo test --workspace`
  are green; only the feature-gated arms break. *(crush-lang-sdk)*
- [x] **portable_vm parity** — `crates/crush-vm/src/portable_vm.rs` was found
  to already implement all 10 opcodes (`PUSH_BOOL`, `NEW_OBJ` / `SET_FIELD`
  / `GET_FIELD`, `ENTER_TRY` / `EXIT_TRY` / `THROW`, `ARR_PUSH` / `ARR_POP`);
  only the parity test surface was missing. Closed by adding 11
  `test_portable_*` tests in `portable_vm.rs::mod tests` mirroring
  canonical `tests.rs` (`EXEC_LANG` gated intentionally — see
  `TICKETS/CRUSHVM-2-EXEC-LANG-POP-NAMED.md` for the pop-on-name followup).
  80 `crush-vm --lib` tests green.

## 🟡 P1 — coverage & language completeness

- [ ] **Test the Rust frontend** — `crush-lang-rust` (`syn` → CAST) ships but has
  **0 tests**. Mirror the `crush-lang-python` test shape (feature detection +
  lowering round-trip).
- [ ] **Test `crush-python` PyO3 bindings** — 0 tests on the cdylib parse path.
- [ ] **Lambda + Match compilation** — currently parse but bail at compile time.
  Wire through `crush-frontend` → CASM.
- [ ] **async/await execution** — parsed but not executable; design the
  fiber/coroutine lowering (see `docs/design/crushvm-rustpython.md` §8 — async →
  Crush fibers is an open question).
- [ ] **Publish core crates to crates.io** — `crush-errors`, `crush-cast`,
  `casm`. External path-dep consumers (openko/fabric, crush-symbols,
  mycelium-mobile, arniko) currently pin via path; publishing unblocks versioned
  deps. *(`workspace = true` already set per the s298 decision.)*

## 🟢 P2 — language frontends (walker maturation)

The `Frontend` abstraction (native parser → CAST) is the active direction; the
remaining tree-sitter/regex walkers are scaffolds (0 tests).

- [x] **JavaScript/TypeScript frontend** (`crush-lang-js`) → **dual-backend**:
  **swc** primary/default (full JS + TS + JSX/TSX, the completeness guarantee) +
  **boa** optional (`boa-backend` feature, JS-only, Boa-aligned). Both lower to one
  CAST. (Walkers are subprocess binaries → swc does NOT land in surfer's graph, so
  its weight isn't a surfer cost; boa exists for a future in-process embedding.)
  Full dispatch-ready spec: **`docs/tasks/js-ts-frontend.md`** (task CA-JS-1).
  Highest-value next frontend; enables "Crush as surfer's script language."
- [x] **Bash frontend** — `bash_walker` is regex-only; migrate to `brush-parser`
  (prior attempt hit an API mismatch). The planned `crush-lang-bash` crate is its
  home.
- [ ] **Go / C / Zig / wasm walkers** — scaffold-level; mature on demand (no pure
  Rust parser alternative for some → may stay tree-sitter).

## 🔗 Cross-project

- [ ] **Tier-3: migrate surfer's Crush runtime → crush-ast** *(consumer-side,
  tracked in surfer-browser + workspace-meta)*. surfer currently runs a
  tree-walk Crush interpreter (relocated from bliss-core in EXO-RB) + an exosphere
  `crush-lang` path-dep. Port it onto crush-ast's bytecode VM (consumes
  `crush_vm::Value` + `HostCaps`). crush-ast main is now stable+pushed = the
  precondition is met.
- [ ] **Reconcile divergence with exosphere's in-tree crush** — exosphere keeps
  its own `crush-lang`/`crush-cast`(1.0.0)/`casm`/`nanovm` and calls crush-ast's
  walker binaries via `SubprocessWalker`. The version drift is a **both-ways
  feature divergence**, not an ancestor relationship — any future merge must
  reconcile feature sets (crush-ast's newer VM types vs exosphere's corecaps
  stdlib / PolyglotContext sandboxing / AI-metadata / Wave3 gating), not just
  bump version numbers.

## 💡 Aspirational (design intent — see `docs/design/crushvm-rustpython.md`)

- [ ] Embedded RustPython lane (RustPython-minimal VM, `crushpy-*` profiles)
- [ ] Subprocess/CPython lane + the three-way lane router
- [ ] `exo.*` capability modules, import firewall, fuel budgets, deterministic
  mode, snapshot/replay
- [ ] Unified capsule-aware GC + the ML "GC policy brain" (advisory-only)

---

## ✅ Done log

- **2026-06-22** — `agent/buffy/CRUSHVM-1-PORTABLE-PARITY` lands 11
  `test_portable_*` parity tests in
  `crates/crush-vm/src/portable_vm.rs::mod tests` mirroring canonical
  `tests.rs` for `PUSH_BOOL`, `NEW_OBJ` / `SET_FIELD` / `GET_FIELD`,
  `ENTER_TRY` / `EXIT_TRY` / `THROW`, `ARR_PUSH` / `ARR_POP`, and the
  `Value::Map` type-name. `EXEC_LANG` followup captured as
  `TICKETS/CRUSHVM-2-EXEC-LANG-POP-NAMED.md`. 80 `crush-vm --lib` tests
  green (was 69). Closes `TASKS.md` 🔴 P0 *portable_vm parity*.
- **s298 (2026-06-16)** — merged `agent/opencode/polyglot` + `agent/opencode/types`
  → main (`edcbe93`); VM type expansion (`Bool`/`Map`/`Error`/`Bytes`) + opcodes
  (ARR_PUSH/POP, NEW_OBJ/SET_FIELD/GET_FIELD, ENTER_TRY/EXIT_TRY/THROW); reconciled
  origin's wasm32 build (#1). 414 tests green. Project registered on the foreman
  radar; `crushpython*.md` scratch consolidated → `docs/design/crushvm-rustpython.md`.
- **2026-06-16** — Python (`rustpython-parser`) + Rust (`syn`) native frontends
  shipped, replacing the old tree-sitter python/rust walkers.
- **s277 (2026-06-12)** — extracted from the exosphere monorepo as a standalone
  workspace.
