# Migration inventory — what is left to move into crush-ast

**Ticket:** CRUSH-150 (derby relay, phase 1 — scout) · **Date:** 2026-10-07 ·
**Baseline:** crush-ast `origin/main` `4034d92` (tag `v0.3.9`) · exosphere `origin/main` `c7ee194c`

This is the baton for phase 2: a builder should be able to pick up a CRUSH-151+ ticket from §5
and start without re-reading the sources. Method: read-only (`git show` / `git ls-tree` /
`git grep` against each source's tracked tree); nothing was built or run unless a row says so.
LOC = `wc -l` of tracked source files (tests counted separately where it matters).

Paths are repo-relative. `ast:` = crush-ast, `exo:` = exosphere, `anc:<repo>/` = the
pre-crush-ast lineage repos (`nixpt/crush` and siblings, all archived/dormant), `caps:` =
`nixpt/crush-capsules`, `sq:` = `nixpt/squeeze`.

## 0. Prior art this builds on (read, not redone)

| Doc | What it settled | Still true at `4034d92`? |
|---|---|---|
| `.jagent/planning/research/2026-09-25-CRUSH-55-delta-inventory.md` | exo front end / CAST / casm / nanovm / vm-runtime / runtimes / crush-common vs ast, per module. Headline: **exo's fork ⊆ ast** for language surface; real ports are engine behaviour behind ast stubs | Yes for the verdicts. Its slice 1 has **landed** (see below) |
| `.jagent/planning/tickets/CRUSH-122.md` | exo `core/base/stdlib` families + nanovm SBL ported (steps 2–4, #61) | Yes — re-verified in §2 |
| `.jagent/planning/tickets/CRUSH-56/57/88–97/108` | archive-zip "137 caps, 103 clean / 46 mock" restoration plan | **Mostly superseded** by CRUSH-122 — see §2.5 |

CRUSH-55 claims re-checked against today's `ast:`:

- Walker binary-name mismatch (`crush_lang_go/zig/wasm`) — **fixed** in #62 (`90fc53f`,
  "go/zig/wasm walker binaries were never found; add PATH fallback"); the `[[bin]]` names are now
  `crush_lang_go`/`crush_lang_zig` (`ast:crates/crush-lang-go/Cargo.toml:9`, `crush-lang-zig/Cargo.toml:10`).
- `SubprocessWalker::resolve_binary` PATH + well-known-dir fallback (CRUSH-55 PORT #3) — **ported**
  (`ast:crates/crush-frontend/src/language_walkers.rs:179`, test `:522`).
- Orphaned `ast:crates/crush-vm/src/polyglot/` (2.2k uncompiled lines) — **deleted** (no longer in
  `ast:crates/crush-vm/src/`).
- AI tool-chain engine + arg plumbing (CRUSH-55 PORT #1) — **still open**: `ai_native.*` caps still
  declare `argc: Some(0)` (`ast:crates/crush-lang-sdk/src/ai_native.rs:87-103`).
- Debugger semantics (CRUSH-55 PORT #2) — **still open**: `crush-debugger` still documents its
  hook points as deliberate `todo!()` (`ast:crates/crush-debugger/src/lib.rs:27`).
- CRUSH-113 — **still open**: `stdlib` is not in `crush-lang-sdk`'s `default` features
  (`ast:crates/crush-lang-sdk/Cargo.toml` `[features]`), and `crush-run --stdlib` without it only
  warns (`ast:crates/crush-lang-sdk/src/bin/crush-run.rs:368-370`).

<!-- §1 module map, §2 stdlib -->

## 3. Runtimes

**exosphere delta since the CRUSH-55 inventory:** none. `git diff --stat 8d52996 c7ee194c` over
`crates/core/vm/{nanovm,casm}`, `platform/sdk/vm-runtime`, `platform/runtimes`, `exo/vortex/src/crush`,
`platform/sdk/capsule-ui/src/crush`, `platform/abi-c`, `core/crush-lang`, `core/crush-cast`,
`core/base/common`, `docs/{runtime,crush}` is empty (the 3 intervening commits are gitignore, CI and
joker-mcp). So CRUSH-55 Appendices B and C stand for exosphere; the table below is the summary plus
the areas it did not cover. Governing decision: **EXO-194 — passive convergence** (exosphere's
in-tree engine is frozen, not migrated; new capsule execution goes through CVM1/exo-light). That means
"migrate into crush-ast" here = *port behaviour crush-ast lacks*, not *move exosphere's consumers*.

| Runtime (source) | LOC | What it does that ast's CVM1 / PortableVm / FastVM / JIT / AOT / crush-web don't | Verdict |
|---|---:|---|---|
| **nanovm** `exo:crates/core/vm/nanovm` | 26.2k src + 8.0k tests/examples | (1) a complete debugger — step into/over/out by frame depth, watchpoints with scopes, event sinks, visibility levels (redacted values), frame snapshots (`src/debug/`, 3.2k), used live by vortex's shell; (2) an AI tool-chain strategy engine (`src/vm/ai.rs`, sequential/parallel/conditional/retry × fail-fast/continue/retry/fallback); (3) in-process Lua (mlua, restricted stdlib) and QuickJS executors; (4) a FastVM host driver that services `call_host`/`exec_lang`/`spawn`/`await` yields; (5) `secure_mem` paged ECASM decryption; (6) `wasi_bridge` value codec; (7) VM pool / lifecycle / supervision (restart, watchdog); (8) Wave3 identity-gated caps | **Adapt concepts, don't copy code.** Port (1) → `crush-debugger` (CRUSH-153), (2) → `crush-lang-sdk::ai_native` (CRUSH-151/152), (3) Lua only, feature-gated, *captain decision* (CRUSH-160). (4) only if FastVM stays a sanctioned engine — lane-guarded `fastvm/` (CRUSH-161, decision). (5)(6)(7)(8) **stay out**: dead in exosphere (5, 6), belong to mandala (7) or exo-light (8, EXO-194 D2) |
| nanovm `sbl_core.{crush,casm}` | 25 + 241 | "System Bytecode Layer" — stdlib bootstrap written in Crush; the `.casm` was a stub (`path_normalize` returned its input) and never wired | **Ported** (CRUSH-122): `ast:crates/crush-lang-sdk/sbl/sbl_core.crush` (19-line diff: adds `format_info` + provenance comment) run by `ast:crates/crush-lang-sdk/src/sbl.rs` — compiled once, each `system.*` call in a fresh quota-bounded PortableVm with only the pure stdlib. vm-runtime's copy is byte-identical to nanovm's |
| **vm-runtime** `exo:crates/platform/sdk/vm-runtime` | ~3.5k own + re-exports | `pub use nanovm::*` facade + unwired `enforcement.rs` (scope wildcards `fs.*`, expiry), stub `CapabilityHandle::call` (→ `Null`), `vm_debug/` (older fork of nanovm `debug`), `capsule_main!` C-ABI macro | **Drop** with nanovm. Two ideas only: scope wildcard + expiry in `Quotas::allowed_caps` (CRUSH-162, optional XS) and redacted debug values (folded into CRUSH-153) |
| **vortex crush** `exo:crates/exo/vortex/src/crush/mod.rs` | 10 | one call to exo `crush_lang::repl::run()` | **Superseded** by `ast:crates/crush-lang-sdk/src/repl.rs` (`run(ReplConfig)`). Exosphere-side repoint, not crush-ast work; vortex's real coupling is its shell debugger on nanovm `Debugger` (`exo:crates/exo/vortex/src/shell/mod.rs`) → blocked on CRUSH-153 |
| **capsule-ui crush** `exo:crates/platform/sdk/capsule-ui/src/crush/` | 248 (tsx/ts) | React `CrushMarkup` that renders arniko-crush HTML | **Not a runtime, not crush-ast's.** Stays in exosphere/arniko. ⚠ renders the raw `html` prop via `dangerouslySetInnerHTML` with no sanitizer (`crush-markup.tsx:175`) — captured for exosphere, not a crush-ast ticket |
| **abi-c** `exo:crates/platform/abi-c` + `include/crush/{capsule.h,capsule_generated.h}` | 719 + 841 headers | capsule-side C ABI; cap entry points return `NotImplemented`; fs/env calls are ambient libc passthroughs; hand-written `capsule.h` declares 17 functions with no implementation | **Drop.** `ast:crates/crush-vm-capi` (embed the VM from C) and `ast:crates/crush-ffi` (plugin ABI) are the sanctioned C surfaces; the ambient calls contradict the cap model |
| **platform/runtimes/*** `exo:crates/platform/runtimes/{python,js,c,go,rust,zig,lua,jvm,swift,ai}` | ~12.5k | per-language execution crates registered as nanovm capabilities | python/js/c/go → **superseded** by `EXEC_LANG` (`ast:crates/crush-vm/src/scheduler.rs`) + buckets (`bucket_exec.rs`) + walkers; rust/zig/jvm/swift/bun → **dead** (never executes / broken / thin unsandboxed subprocess); lua → CRUSH-160 decision; **ai → port engine only** (CRUSH-151/152). One design idea worth a ticket: python worker's guest→host callback bridge protocol (CRUSH-163, design note) |
| **docs** `exo:docs/crush/EXO-205-divergence-inventory.md`, `exo:docs/runtime/exo-92-stub-audit.md` | 313 + 167 | exosphere-side divergence inventory; packman/nanovm stub audit | **Superseded** by CRUSH-55 (EXO-205) / historical (EXO-92). Stay in exosphere |

**crush-ast runtimes, for contrast** (what already exists and is the go-forward home): CVM1 scheduler
(`crush-vm/src/scheduler.rs`, production), PortableVm (`portable_vm.rs`, debugger/web, pause-before-
instruction for host-fed caps), FastVM (`fastvm/`, alpha; `run_fastvm_with_caps` uses `DummyHal` and
services nothing but AI/DOM stubs — `vm.rs:774-794`), JIT (`crush-jit`, FastVM fallback), AOT Rust/C
(`crush-aot`, compute-only), AOT-C (`crush-aotc`, orphan), PTX (`crush-ptx`), browser (`crush-web`,
`Session`/`execute_with`). Nothing in any exosphere runtime is a *new execution tier* crush-ast lacks;
the gaps are debugger semantics, AI engine, and (optionally) in-process Lua.

**Opcode delta (nanovm/exo casm → CVM1)**, for completeness: nanovm-only string ops are
`export_var/import_var`, `call_host` (with `ic_id`), `call_interface`, `new_struct`, `gc`,
`push_const_array`, casts `int/float/bool/string`, `ai_adaptation_request`, `ai_capability_discovery`,
and the AI spellings `ai_goal_decl`/`ai_knowledge_share`/`ai_tool_chain` (ast: `ai_goal_declaration`/
`ai_knowledge_sharing`/`ai_toolchain`). None carries semantics worth porting except as FastVM-lowerer
aliases, which only matter if exo-emitted CASM is ever fed to ast's FastVM (folded into CRUSH-161).
Value model: nanovm `RuntimeValue` is byte-identical to `ast:crates/crush-vm/src/value.rs`; CVM1's
`Value` (`vm.rs:128`) is a strict superset of nanovm's heap objects.

**Exosphere's live pins on crush-ast** (for whoever repoints a consumer): only exo-light, via crates.io
`crush-vm = "0.3.6"` (`exo-light/Cargo.toml:36`). exosphere's `Cargo.lock` therefore already holds
both its in-tree `casm 0.1.0` and crates.io `casm 0.3.0` — EXO-194's "rename before both casms meet"
guard has been overtaken (harmless while the versions differ). ~20 exosphere crates still path-dep
nanovm; that is exosphere's to retire, not a crush-ast task.

<!-- §4-6 -->
