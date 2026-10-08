# Migration inventory — what is left to move into crush-ast

**Ticket:** CRUSH-150 (derby relay, phase 1 — scout) · **Date:** 2026-10-07 ·
**Baseline:** crush-ast `origin/main` `4034d92` (tag `v0.3.9`; `be3ed23` since adds only CRUSH-118) · exosphere `origin/main` `c7ee194c`

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
- Debugger semantics (CRUSH-55 PORT #2) — **still open**: the `todo!()` panics are gone
  (`ast:crates/crush-debugger/src/session.rs:346` replaced them), but there is no step-over/out or
  watchpoint anywhere in `crush-debugger/src` — only bytecode breakpoints (`portable_vm.rs:121-279`).
  Its `README.md:11,43` and `lib.rs:27` still describe the old `todo!()` hook points (stale).
- CRUSH-113 — **still open**: `stdlib` is not in `crush-lang-sdk`'s `default` features
  (`ast:crates/crush-lang-sdk/Cargo.toml` `[features]`), and `crush-run --stdlib` without it only
  warns (`ast:crates/crush-lang-sdk/src/bin/crush-run.rs:368-370`).

## 1. Module map

Status vocabulary: **ported** (ast has it — evidence path given) · **partial** (what's missing named) ·
**superseded** (by what) · **unique** (worth porting) · **dead** (why) · **out** (not Crush; belongs
elsewhere). The "→" column names the phase-2 ticket (§5) or "—". Effort S/M/L, risk L/M/H.

### 1a. exosphere in-tree Crush (`exo:` `c7ee194c`)

Zero delta since CRUSH-55's base `8d52996` for every path below, so per-file detail is in that
inventory's Appendices A–C; one row per module group here.

| Source | What | LOC | ast equivalent | Status | → | Eff | Risk |
|---|---|---:|---|---|---|---|---|
| `crates/core/crush-lang` | parser/semantics/optimizer/compiler, imports, walkers registry, REPL | ~9k | `crates/crush-frontend`, `crush-lang-sdk/src/repl.rs` | **superseded** — zero exo-only fn/token/node (CRUSH-55 App. A); walker PATH fallback already ported (`crush-frontend/src/language_walkers.rs:179`) | — | — | L |
| `crates/core/crush-cast` (1.0.0) | CAST IR | ~5k | `crates/crush-cast` | **superseded** (strict subset; `ai_meta` on both) | — | — | L |
| `crates/core/vm/casm` | CASM format | ~1.5k | `crates/casm` | **superseded** (65 vs 113 ops, 0 exo-only) | — | — | L |
| `crates/core/vm/casm/src/ecasm.rs` | encrypted CASM pages | 1,014 | none (deleted by CRUSH-80) | **out** — live exo consumer (`ant crush encrypt`, `.ecap` loader); stays exo-owned | — | — | — |
| `crates/core/base/errors` | error types | 937 | `crates/crush-errors` | **ported** (byte-identical modulo `ResultExt`, 0 callers) | — | — | L |
| `crates/core/vm/nanovm/src/debug/` | step into/over/out, watchpoints, event sinks, visibility, snapshots | 3,166 | `crates/crush-debugger` (breakpoints + REPL only) | **unique** — re-implement over `PortableVm`, don't copy | 159, 160 | M | M |
| `crates/core/vm/nanovm/src/vm/ai.rs` + `platform/runtimes/ai/src/{toolchain,delegation}.rs` | AI tool-chain strategies; delegation selection | 596 + 898 | `crush-lang-sdk/src/ai_native.rs` (argc-0 echo stubs) | **unique (engine only)**; backends (joker-mcp, foreman-dispatch, box paths) **out** | 156–158 | M | M |
| `crates/core/vm/nanovm/src/polyglot` (Lua/QuickJS in-process) | restricted-stdlib Lua, JS console capture | 2,282 | `EXEC_LANG` subprocess + buckets; no Lua | **unique (Lua only)**, captain decision | 162 | M | M |
| `crates/core/vm/nanovm/src/vm/mod.rs:932-1080` | FastVM host driver servicing yields | ~150 | `crush-vm/src/vm.rs:774-794` (`DummyHal`) | **unique**, only if FastVM stays sanctioned (lane-guarded) | 163 | M | M |
| `crates/core/vm/nanovm/src/audit.rs` | instruction transcript hash | 288 | none | **unique (idea)** — `record_cap_call` never wired; design fresh in CVM1 | 165 | S | L |
| `crates/core/vm/nanovm/src/{fastvm,memory,value,registry,traits,ai_optimizer}` | FastVM, arena, values, cap registry | ~6.9k | `crush-vm/src/{fastvm/,memory.rs,value.rs,host.rs,caps.rs,ai_optimizer/}` | **superseded** (`value.rs` identical; ast ahead elsewhere) | — | — | L |
| `crates/core/vm/nanovm/src/{secure_mem,wasi_bridge,bytecode,runtime,capsule,interface,codec,events,ipc}` + `RichValue` | encrypted exec, CBV codec, parallel bytecode, dead runtime hooks | ~6.0k | none | **dead** (never enabled / no consumer / parallel unused format) | — | — | — |
| `crates/core/vm/nanovm/src/{lifecycle,pool,task,phases}` | VM lifecycle, pool, supervision, phase metrics | ~4.5k | `scheduler.rs` green threads | **out** → antarikshya mandala/x-ray (`phases` metrics are fabricated) | — | — | — |
| `crates/core/vm/nanovm/src/sbl_core.{crush,casm}` | System Bytecode Layer | 266 | `crush-lang-sdk/sbl/sbl_core.crush`, `src/sbl.rs` | **ported** (CRUSH-122). `.casm`'s `fs_cp` not carried — superseded by CRUSH-151's `fs.cp` | — | — | L |
| `crates/core/vm/nanovm/{examples,models}` | phase demos; ONNX GC model checksum (model file absent) | 5,128 + 52 | — | **dead** (`autoexamples = false`, bitrotted; model missing) | — | — | — |
| `crates/platform/sdk/vm-runtime` | nanovm facade + unwired enforcement/debug forks | ~3.5k | `crush-vm` | **superseded**; ideas → 160 (redaction), 164 (wildcards) | 160, 164 | — | L |
| `crates/platform/runtimes/{python,js,c,go}` | per-language runtimes | ~7.9k | `EXEC_LANG` + `bucket_exec.rs` + walkers | **superseded**; python worker bridge protocol → design note | 166 | S | L |
| `crates/platform/runtimes/{rust,zig,jvm,swift,js/bun}` | thin / non-executing runtimes | ~2.7k | `cargo_cap.rs`, walkers | **dead** (never runs binary / no WASI imports / unsandboxed subprocess) | — | — | — |
| `crates/platform/runtimes/lua` | mlua runtime, no-op sandbox | 391 | none | **dead as-is**; see 162 | 162 | — | — |
| `crates/core/base/common` (crush-common) | HAL traits, ICBF, capsule ABI/lifecycle, event loop | 3,498 | `HostCap`/`HostCapSpec` | **superseded** (icbf, host_dispatch) / **out** (abi, capability, lifecycle → capsule-contract; event_loop → exo-hal) | — | — | — |
| `crates/platform/sdk/crush-sdk` (+ `wave3.rs`) | Rust capsule SDK on stub `CapabilityHandle::call` | 737 | none needed | **out** (capsule-contract replaces) | — | — | — |
| `crates/platform/abi-c` + `include/crush/*.h` | capsule-side C ABI | 719 + 841 | `crush-vm-capi`, `crush-ffi` | **dead** (cap entry points `NotImplemented`; ambient libc; 17 declared-unimplemented fns) | — | — | — |
| `crates/exo/vortex/src/crush/mod.rs` | REPL shim | 10 | `crush-lang-sdk/src/repl.rs` | **superseded** (exo-side repoint) | — | — | — |
| `crates/platform/sdk/capsule-ui/src/crush/` | React `CrushMarkup` renderer | 248 | none | **out** (UI host); unsanitized HTML injection captured for exosphere | — | — | H (exo) |
| `crates/core/base/stdlib`, `archive/archived-stdlib`, `tests/stdlib`, `crates/capabilities/corecaps`, `crates/exo/core-utils` | stdlib / stdcap / corecap | see §2 | `crush-lang-sdk/src/stdlib*` | see §2 | 151–155 | | |
| `docs/crush/EXO-205-*.md`, `docs/runtime/exo-92-*.md` | divergence inventory; stub audit | 480 | CRUSH-55 inventory | **superseded** / historical (stay in exo) | — | — | — |

### 1b. `nixpt/crush` and `nixpt/crush-language` (`anc:crush/`, archived 2026-05-17)

Dual MIT/Apache (same as ast). Dormant banner: canonical pieces went to exosphere (`crates/core/*`,
`crates/exo/*`, `crates/platform/sdk/crush-sdk`, `docs/crushed-book`). `crush-language` is a README only.
The core crates form a dependency cycle (`crush-lang` ↔ `nanovm`, both → `exo-core`): nothing lifts out
without rewiring, so everything below is port-by-reimplementation.

| Source | What | LOC | ast equivalent | Status | → | Eff | Risk |
|---|---|---:|---|---|---|---|---|
| `core/crush-errors`, `core/casm` (+ `specs/casm_v1.0.md`) | errors, CASM | ~1.5k | `crates/crush-errors`, `crates/casm` (spec byte-identical) | **ported** | — | — | L |
| `core/casm/src/{ecasm,polyglot}.rs` | encrypted CASM; "enhanced" polyglot op set | 1,613 | CVM1 `opcodes.json`, `EXEC_LANG` | **dead** / **superseded** | — | — | — |
| `core/crush-lang` (compiler, semantics, optimizer, imports, `ai_ast`, `ai_native`, specs, `walkers/go_walker`) | front end | ~6.1k | `crates/crush-frontend` (2–3× larger), `crush-cast/src/ai.rs`, `crush-frontend/src/ai_runtime.rs`, `crates/crush-lang-go` | **superseded** (ast CAST is a strict superset incl. `Lambda`, `Match`) | — | — | L |
| `core/nanovm` (vm, scheduler, fast_vm, arena, registry, …) | VM | ~7.7k | `crates/crush-vm` | **superseded**; supervision types/tests have no ast equivalent → fold into 163 | 163 | — | M |
| `core/nanovm/src/debug/` | debugger engine | 1,880 | `crates/crush-debugger` | **unique** (older copy of exo's; use exo's as reference) | 159, 160 | M | M |
| `core/nanovm/src/audit.rs` | hash-chained instruction/event log | 99 + 133 test | none | **unique (small)** | 165 | S | L |
| `core/nanovm/src/{secure_mem,wasi_bridge,native_runtime}.rs`, `vm.rs_stub`, `specs/vm_v1.0.md` | | ~2.6k | `crush-web`, `EXEC_LANG` | **dead** / **superseded** | — | — | — |
| `core/crush-stdlib` pure families (`binary bytes buffer result text math`) | | ~1.5k | `crush-lang-sdk/src/stdlib/*` | **ported** (= exo `archived-stdlib`) | — | — | L |
| `core/crush-stdlib/src/{fs,ics}.rs` (`fs.ls/cp/mv/mkdir/find/cd/pwd/cat/touch`) | coreutils fs caps | ~660 | `fs.read/write/exists/list` only | **unique** — 3 ast examples are `expect-error` on these (`examples/crush/{fs_test,repl_test,phase2_3_test}.crush`) | 151 | M | M |
| `core/crush-stdlib/src/async_cap.rs` | `async.sleep` | 62 | `time.sleep` | **unique (alias)** — `examples/crush/async_test.crush` is `expect-error: async.sleep` | 152 | XS | L |
| `core/crush-stdlib/src/{storage,ai_capabilities,polyglot_bridge,task,gfx,missing_capabilities,implementation_plan}.rs` | | ~3.6k | see §2 | see §2 (exo's live copy is the newer source) | 154 | | |
| `core/crush-common` | HAL, event loops, ICBF | ~3.1k | `crush-vm/src/{caps,host}.rs`, `crates/crush-net` | **superseded** / **out** | — | — | — |
| `core/{exo-core,mesh,exo-metrics,nexus-privacy}`, `infra/*`, `examples/acp-ai-client`, `web/crush-web-dashboard` | exokernel, libp2p mesh, metrics, browser privacy, ops UI | ~58k | `crates/crush-net` replaces mesh; `crush-pkg/src/{ecap,signer,merkle}.rs` replace signing | **out** (exosphere) / **dead** | — | — | — |
| `tools/crush-cli` | old `crush` CLI | 5,131 | `crush-lang-sdk` bins, `crush-pkg`, `crush-installer`, `crush-tui` | **superseded**, except `doctor`'s polyglot-runtime checks | 175 | S | L |
| `web/crush-web` (`browser_capsules.rs`) | WASM runtime + PWA capsule loader | 998 | `crates/crush-web` (`execute`, `run_blob`, `Session`) | **partial** — browser capsule manifest/permissions only; no consumer yet | — | M | M |
| `web/crush-web-ide` | browser IDE/playground shell | ~1.9k | none in-repo | **unique** — check foreman's crush-web playground work first (CRUSH-118 follow-up) | 174 | M | M |
| `tests/` | `.crush`/`.cast`/`.casm` goldens | 1.1k rs + 12k golden | `examples/crush/*` (11/11 `.crush` present) | **ported** (`.crush`); old-format goldens **dead** | — | — | — |
| `benchmarks/` | "benchmarks" | 3,508 | `benches/`, `docs/benchmarks/` | **dead** (`tokio::time::sleep` simulations) | — | — | — |
| `examples/{types,memory}/*.crush` | classes, pointers | ~600 | none | **dead** — aspirational syntax the compiler never supported | — | — | — |
| `docs/architecture/import-system.md` | import system spec | 296 | none in-repo (impl: `crush-frontend/src/import_system.rs`) | **unique (doc)** — re-verify against impl (CRUSH-110: `import` is a no-op) | 173 | S | L |
| `infra/deploy/docker` | Dockerfile/compose | 108 | none | **unique, low value** | — | S | L |

### 1c. Other ancestors (`anc:crush-sdk`, `crushed-book`, `walker`, `joker-coordinator`, `joker-protocol`)

Licences: only `joker-protocol` has one (MIT, with an unusual copyright line); `crush-sdk` declares MIT
in `py/Cargo.toml`; `crushed-book`, `walker`, `joker-coordinator` have **none** → rewrite, don't copy
text. Several files contain personal absolute paths — scrub anything that moves.

| Source | What | LOC | ast equivalent | Status | → |
|---|---|---:|---|---|---|
| `crush-sdk/{rust,abi-c,py,js}` | capsule-authoring SDK + C ABI | ~5.2k | exo `capsule-sdk`/`crush-sdk`/`abi-c` | **out** (absorbed by exosphere) | — |
| `crush-sdk/{go,zig,swift,jvm,c}` | stub SDKs | ~1.7k | none | **dead** | — |
| `crush-sdk/assets/icons` (`crush.svg`, `casm.svg`) | file icons | — | none in `crush-workspace/crush-vscode` | **unique** — crush-vscode, not crush-ast (licence check first) | — |
| `crushed-book/src/reference/{crush,casm,appendix}` | language reference | ~5.2k | `crush-workspace/crush-language-guide/src/*` | **ported** (guide is newer) | — |
| `crushed-book/src/reference/advanced/{compilation,walkers}.md` | pipeline; walker authoring | 138 | none / `crush-walker-core/README.md` | **unique (rewrite)** against real `crushc` pipeline and `Frontend`/`LanguageAdapter` | 173 |
| `crushed-book/src/{configuration,nanovm,stdlib_*,getting_started,installation,cli_usage}.md`, `encrypted_execution.md` | | ~1.1k | — | **dead — overclaims** (no `~/.crush/config.json`, no packman/`exo` CLI, cap names drift from `host_caps.rs`) | — |
| `crushed-book/src/{vision,architecture,exo_core,vortex,…}` | Exosphere platform | ~1.4k | — | **out** (exosphere `docs/crushed-book` is canonical and a superset) | — |
| `walker/*` | `walker-core`, 7 walkers, cli, tree-sitter-crush | ~4.2k (+12.5k generated) | `crush-walker-core`, `crush-lang-*`, `crates/cli`, `crates/tree-sitter-crush` | **superseded** (every ast walker ≥ ancestor; Go identical node set) | — |
| `joker-coordinator/*` | agent coordinator; capsule-registry/runner | ~38.6k (60% nested dupes) | `crush-pkg/src/{ecap,runners}.rs` | **out** (→ exo joker-core) / **dead** (registry/runner never compiled) | — |
| `joker-protocol/*` | MCP server, agent tooling, specs | ~23.9k | — | **out** (→ exo joker-mcp, `ai-protocols/`) | — |
| `joker-protocol/docs/{research_casm_utilities,sbl_corecap_integration}.md`, `specs/rfcs/RFC-029-*` | SBL/corecap layering; WIT idea | ~150 | `crush-lang-sdk/src/sbl.rs` | **unique (design-note summary)** | 173 |

### 1d. `nixpt/crush-capsules` (`caps:` `174c934`)

Almost all of it targets exosphere's in-process `crush-sdk`/`nanovm` capsule model, not crush-ast's
file-manifest capsules (`crush-pkg`). Its own `SALVAGE_NOTES.md` names that split as the open question.

| Source | What | LOC | ast equivalent | Status | → |
|---|---|---:|---|---|---|
| `games/snake`, `games/turtle-runner` | games | 3.1k | `examples/crush/snake.crush`, `examples/js-walked/turtle_runner.js` | **ported** (headers say so; `examples/README.md:21,40`) | — |
| `squad-bridge-peek/{main.crush,capsule.toml}` | first pure-Crush capsule (`fs.cat`) | 52 | `crush-pkg` manifest shape matches | **unique (XS)** — needs `fs.cat` (CRUSH-151) and a scrubbed path | 172 |
| `services/sqlite-provider` | SQLite cap provider on exo SDK | 1,281 | `crush-lang-sdk` `db` feature | **superseded** for ast (alive exo-side) | — |
| `tui/teddy` | editor | 4,281 | moved to `nixpt/teddy` 2026-07-15 | **dead dup** here | — |
| `hub.json`, `CAPSULE_CATEGORIES.md`, `STANDARDS` | category/platform metadata | — | `crush-pkg` `Manifest` has none | **unique (idea)** | 171 |
| `tools/*`, `learn`, `archive/RFC`, `templates`, `demos/*`, `examples/*`, `standalone/*`, `demo/hello-wave3` | coreutil/agent/demo capsules on missing paths | ~10k | `crush-pkg new`/`runners.rs` | **dead** (path-deps don't resolve; made-up traits) | — |
| `gui/*`, `webui/*`, `vscode-exosphere` | Tauri apps, Monaco editor, Joker VS Code ext | ~35k | — | **out** (exosphere/Joker UI) | — |
| `docs/universal-capsule-design.md` | native/browser/WASI capsule design | 844 | `crush-web` + `crush-pkg` site | **idea only** | — |
| `system/` (broken symlink, committed ELF cache) | | — | — | **junk** | — |

### 1e. Archives (`_rama-archive`, `incubator`)

`_rama-archive`: 41 hits, all OS-level "capsules" (namespaces/cgroups/zram) — **not Crush**, nothing to
move. `incubator` is a **local-only, private** repo (raw agent transcripts) — summarized, nothing copied:

| Source | What | LOC | Status | → |
|---|---|---:|---|---|
| `parked/crates/ai/services/shadow-capsule` | infers required caps from a CAST program's imports/calls and diffs against declared caps | 595 | **unique (idea)** — no capability inference in ast; fits `crush-pkg check` / crush-lint | 170 |
| `parked/crates/ai/services/aspect-weaver` | before/after injection over CAST | ~260 | **dead** (curiosity) | — |
| `parked/crates/services/{capsule-runner,registry/app-registry}` | | ~400 | **superseded** (`crush-pkg/src/runners.rs`; exo `app-registry`) | — |
| `recovered/ai-native-lsp-server-for-crush.recovered.md` | Crush LSP design | 442 | **superseded** by peer repo `crush-workspace/crush-lsp` | — |
| `recovered/*` (other), `prim_linux*` | exosphere specs, transcripts | — | **out** / private | — |

## 2. stdlib, stdcap, corecap

### 2.1 Definitions (from the sources)

- **stdcap** — a *classification label*, not a crate: pure, side-effect-free utility capabilities that
  need no grant. exosphere uses it as section comments in `exo:crates/core/base/stdlib/src/lib.rs`
  ("stdcap: str") and as the "Kind" column of `exo:crates/capabilities/corecaps/src/lib.rs`;
  `exo:crates/core/crush-cast/STATUS.md` lists the members: *str / collections / math / json / conv /
  path / regex / bytes / buffer / binary / result / data*. crush-ast adopted the word
  (`ast:crates/crush-lang-sdk/src/stdlib.rs` header: "These are stdcaps — always available, no
  capability gate required" — **inaccurate today**, see §2.4).
- **corecap** — two meanings; keep them apart:
  1. *(current, label)* system-access capability namespaces: STATUS.md lists *env / time / http / fs /
     text / storage / task / gfx / ai / agent / learn / async / polyglot / python / js / dom*. The crate
     `exo:crates/capabilities/corecaps` (lib.rs 482 L, workspace member, created 2026-02-15) contains
     **no implementations**: `register_corecaps()` re-registers every `stdlib::*` type (28 namespaces,
     179 caps, stdcap + corecap) into a `nanovm::Registry` — and **nothing calls it**. Live exosphere
     consumers (`exo/cli/src/loader_bridge.rs`, `exo/runtime-core/src/runners.rs`,
     `exo/vortex/src/shell/mod.rs`) call `stdlib::create_std_registry` instead.
  2. *(older, crate)* `exo:crates/exo/core-utils` (package name `corecap`, ~2.2k L): capability-aware
     coreutils (`ls cat stat chmod chown mkdir mv pwd rm`) on an `ExoUtility` trait, injected into the
     vortex shell (v1.2 SHELL-02). The 2026-01-22 proposal
     `exo:crates/ai/core/protocol/ai-protocols/docs/sbl_corecap_integration.md` layers it as
     HAL → SBL (CASM) → stdlib → corecap(2) and plans to rewrite corecap in Crush; only the stub
     `sbl_core.casm` ever existed.
- **SBL** (System Bytecode Layer) — stdlib bootstrap written in Crush (`exo:crates/core/vm/nanovm/src/
  sbl_core.{crush,casm}`). Ported (CRUSH-122) as `system.*`.
- **Grants** — osmosis's `HostCapability` (`name()` + `call(Vec<HostValue>)`, gated by
  `CapabilityAuthority::authorize`) has no fixed catalogue: `OsContext::grant(name)` accepts any Crush
  permission string validated by capsule-contract's `CapabilitySet::from_crush_permissions`. `fs.*` /
  `store.*` / `net.*` are convention. Note osmosis's `HostValue` is **scalar-only**, so families that
  return arrays/maps cannot cross the osmosis ABI — another reason pure families stay in-VM.

**Working rule** (consistent with CRUSH-122 and the capability model): *stdcap → in-VM, no grant,
in `crush-lang-sdk`'s stdlib. corecap(1) → host capability behind a named grant (`--fs`, `--time`,
`--env`, `net` feature, …). Execution semantics (scheduling, watchdog, polyglot) → crush-vm.*
corecap(2) is a shell-utility layer and is **out** — its useful surface is the fs family below.

### 2.2 Family classification (every family found in any source)

| Family | Kind | Where in ast today | Gap → ticket |
|---|---|---|---|
| `str` (18), `collections` (16), `conv` (7), `json` (3), `path` (7), `regex` (5), `math` (15 incl. random/seed) | stdcap | `crush-lang-sdk/src/stdlib.rs`, `stdlib/collections_ext.rs` | none |
| `bytes`, `buffer`, `binary` (12), `result` | stdcap | `stdlib/{bytes,binary,result}.rs` | none |
| `text.sort/uniq`, `time.format/parse`, `env.os/arch`, `system.*` (SBL) | stdcap | `stdlib/{text,time_fmt,env_info}.rs`, `src/sbl.rs` | none |
| `data.parse_json/filter/map/groupby/reduce` | labelled stdcap, but a toy | — | **drop** (only named predicates; unknown ones return the input silently). Closures/loops + `json.parse` supersede |
| `fs.read/write/exists/list` | corecap (fs grant) | `crush-lang-sdk/src/host_caps.rs` (`--fs`, `--fs-root` sandbox) | — |
| `fs.ls/cat/pwd/mkdir/rm/cp/mv/touch/find` | corecap (fs grant) | **missing** | **CRUSH-151** (`fs.cd` declined: process-wide state; see decision C-5) |
| `text.head/tail/wc/cut/grep` | corecap (fs grant) | `src/text_tools.rs` | none |
| `time.now/now_ms/now_iso/elapsed/sleep` | corecap (time grant) | `host_caps.rs` (`--time`). ⚠ `time.now` = seconds in ast, ms in exo | none (document the unit) |
| `async.sleep` | corecap (time grant) | missing; `time.sleep` exists | **CRUSH-152** (alias) |
| `env.get` | corecap (env grant) | `host_caps.rs` (`--env`) | — |
| `env.all`, `env.home_dir` | corecap (env grant) | missing | **CRUSH-153** |
| `http.get/post` ≈ `net.http_get/post` | corecap (net grant) | `src/net.rs` (`net` feature); raw TCP/TLS in `crates/crush-net` | — |
| `http.put/delete/request` | corecap (net grant) | missing | **CRUSH-153** |
| `storage.open/read/write/size/close` | corecap (store grant), handle-based | missing; ast has `db.query/execute` (`db` feature) | **CRUSH-154** — decision C-6 (port vs decline in favour of `db.*`) |
| `process.*`, `crypto.*` | corecap | ast-only (`host_caps.rs`) | — |
| `ai.*`, `agent.*`, `learn.*`, `ai.embed/tokenize` | corecap → ai-core host | `ai_native.*` stubs | engine only: **CRUSH-156–158**; exo impls are mocks (`ai.embed` from char codes, `agent.spawn` mock id) → dead |
| `dom.*` (21), `gfx.*` | corecap → UI host (surfer/arniko) | `dom_native.*` stubs, `graphics.*` (SVG) | **out** (exo impls are placeholders returning `Int(0)`/Null) |
| `polyglot.lib/call/transfer`, `python.stdlib`, `js.stdlib` | corecap | `polyglot.<lang>` gates + `EXEC_LANG` | **dead** (mock values) |
| `task.restart/watchdog` | crush-vm (opcode) | FastVM lowers `watchdog`/`restart`; nothing services them | folded into **CRUSH-163**; exo caps always error → dead |
| `ics` (IC records), `EffectRecord` | metadata | `HostCapSpec` (name/argc/returns), no effects | **CRUSH-155** (optional: `effects` on `HostCapSpec`) |
| `print`, `text.echo` | duplicates | `io.print` | **dead** |

### 2.3 CRUSH-122 re-verification (against `4034d92`)

Every cap CRUSH-122 says landed exists under the stated name (collections_ext 9, bytes/buffer 8, binary
12, result 4, `text.*`, `time.*`, `env.os/arch`, `system.*`); `PortableVm::push_entry_args`
(`portable_vm.rs:251`) and `Value::type_name` (`vm.rs:194`) are public; `crush-frontend/tests/
array_intrinsics.rs` exists. exosphere's stdlib, archived-stdlib, tests/stdlib, corecaps and SBL files
are **unchanged** since CRUSH-122's source commit `06b68057`. Deltas:

1. Counts drifted: `stdlib.rs` is 2,569 lines (ticket: 2,523); math 15 / str 18 (ticket's pre-port
   table: 21 / 16). CRUSH-113's `crush-run.rs:335` is now `:368-370`.
2. "SBL never wired up" is true for *crush-ast's* sense only: exo-cli's `register_sbl_into_registry`
   (`exo:crates/exo/cli/src/loader_bridge.rs`, called from `handlers/run.rs:81`) loads the stub `.casm`
   from home/cwd paths. The `.casm`'s `fs_cp` (over `storage.open`) was not ported — superseded by
   CRUSH-151.
3. Missed by CRUSH-122's "homes recorded": `env.all`, `env.home_dir`, `http.put/delete/request` (→ 153);
   the 6 `crush_ai_runtime` caps (`ai.query`, `ai.agent_delegation`, `ai.goal_declaration`,
   `ai.progress_update`, `ai.knowledge_sharing`, `ai.adaptation_request`) that `create_std_registry`
   registers (→ 156–158); `EffectRecord` metadata (→ 155).
4. `archive/archived-stdlib` is an older snapshot of the live crate (not built, unresolvable paths);
   its only extra content, an uncompiled `create_std_registry.rs` naming undefined `io.*`/`crypto.*`/
   `system.*`, is covered by ast's `crypto.*`, crush-net and `env.*`. **Dead.**
5. `exo:tests/stdlib`: the `.crush` files are already in `examples/crush/` (`math_test`, `logging_test`,
   `text_tools_test`); the Rust tests reference a commented-out import and only `println!` on failure.
   **Dead.**

### 2.4 CRUSH-113 — current state and proposed default

Still **off by default**: `crush-lang-sdk`'s `default = ["native-plugins", "polyglot-python",
"polyglot-javascript"]`. `pub mod stdlib` always compiles, but registration (`HostCapsBuilder::
stdlib(true)`) exists only under `cfg(feature = "stdlib")`. Without it: `crush-run --stdlib` prints a
warning and continues (`bin/crush-run.rs:368-370`); the first stdlib cap call fails as `VmError::
UnknownCap` → "[runtime] unknown capability: <name>" and exit 1; `crush-repl` silently ignores
`config.stdlib` (`repl.rs:420,488`). Unverified caveat: `crush-lang-js` and `xtask` enable `stdlib`,
so workspace-wide builds may unify it on and hide the gap in tests.

**Proposal: default-on, and stop gating stdcaps behind `--stdlib` at runtime.** The stdlib is pure
(stdcap — no I/O, nothing to grant), its only extra dependency is `regex` (already compiled in workspace
builds: `crush-lang-js/Cargo.toml:37` and `xtask` enable `stdlib`), and the capability model's promise is "no *ambient authority*", which pure functions
do not confer. Keep the cargo feature so `crush-web`/embedded builds can opt out, but make a missing
feature a **hard error** when `--stdlib` (or a stdlib cap) is requested, and fix the `stdlib.rs` header.
I/O halves (`text.head…`, `time.now…`, `fs.*`) stay behind their grants. This is decision **C-1**; the
implementation is the existing CRUSH-113 ticket (S, ~20 turns), placed first in the relay (§5).

### 2.5 The archive-zip restoration tickets are superseded

`CRUSH-56` (tracker), `CRUSH-57` (46 mock-tainted caps), `CRUSH-88..97` (ten identical shard templates
with no cap lists) and `CRUSH-108` (reconcile source) planned a restore from `exosphere-1.0.zip`
(present at a local scratch path, 700 MB). That zip's `crates/core/base/stdlib` is the same crate as
exosphere's live tree, and its `archive/archived-stdlib` is the older snapshot; CRUSH-122 already
restored the clean families **from the live tree**, and the 46 "mock-tainted" caps map onto exactly the
families classified *dead/out* above (polyglot, ai/agent/learn, dom, task). The genuine remainder is
CRUSH-151–155. **Recommend closing CRUSH-56/57/88–97/108 as superseded by CRUSH-122 + this
inventory** (foreman's call — I have only added pointers).



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
| **nanovm** `exo:crates/core/vm/nanovm` | 26.2k src + 8.0k tests/examples | (1) a complete debugger — step into/over/out by frame depth, watchpoints with scopes, event sinks, visibility levels (redacted values), frame snapshots (`src/debug/`, 3.2k), used live by vortex's shell; (2) an AI tool-chain strategy engine (`src/vm/ai.rs`, sequential/parallel/conditional/retry × fail-fast/continue/retry/fallback); (3) in-process Lua (mlua, restricted stdlib) and QuickJS executors; (4) a FastVM host driver that services `call_host`/`exec_lang`/`spawn`/`await` yields; (5) `secure_mem` paged ECASM decryption; (6) `wasi_bridge` value codec; (7) VM pool / lifecycle / supervision (restart, watchdog); (8) Wave3 identity-gated caps | **Adapt concepts, don't copy code.** Port (1) → `crush-debugger` (CRUSH-159/160), (2) → `crush-lang-sdk::ai_native` (CRUSH-156–158), (3) Lua only, feature-gated, *captain decision* (CRUSH-162). (4) only if FastVM stays a sanctioned engine — lane-guarded `fastvm/` (CRUSH-163, decision). (5)(6)(7)(8) **stay out**: dead in exosphere (5, 6), belong to mandala (7) or exo-light (8, EXO-194 D2) |
| nanovm `sbl_core.{crush,casm}` | 25 + 241 | "System Bytecode Layer" — stdlib bootstrap written in Crush; the `.casm` was a stub (`path_normalize` returned its input) and never wired | **Ported** (CRUSH-122): `ast:crates/crush-lang-sdk/sbl/sbl_core.crush` (19-line diff: adds `format_info` + provenance comment) run by `ast:crates/crush-lang-sdk/src/sbl.rs` — compiled once, each `system.*` call in a fresh quota-bounded PortableVm with only the pure stdlib. vm-runtime's copy is byte-identical to nanovm's |
| **vm-runtime** `exo:crates/platform/sdk/vm-runtime` | ~3.5k own + re-exports | `pub use nanovm::*` facade + unwired `enforcement.rs` (scope wildcards `fs.*`, expiry), stub `CapabilityHandle::call` (→ `Null`), `vm_debug/` (older fork of nanovm `debug`), `capsule_main!` C-ABI macro | **Drop** with nanovm. Two ideas only: scope wildcard + expiry in `Quotas::allowed_caps` (CRUSH-164, optional XS) and redacted debug values (folded into CRUSH-160) |
| **vortex crush** `exo:crates/exo/vortex/src/crush/mod.rs` | 10 | one call to exo `crush_lang::repl::run()` | **Superseded** by `ast:crates/crush-lang-sdk/src/repl.rs` (`run(ReplConfig)`). Exosphere-side repoint, not crush-ast work; vortex's real coupling is its shell debugger on nanovm `Debugger` (`exo:crates/exo/vortex/src/shell/mod.rs`) → blocked on CRUSH-159 |
| **capsule-ui crush** `exo:crates/platform/sdk/capsule-ui/src/crush/` | 248 (tsx/ts) | React `CrushMarkup` that renders arniko-crush HTML | **Not a runtime, not crush-ast's.** Stays in exosphere/arniko. ⚠ renders the raw `html` prop via `dangerouslySetInnerHTML` with no sanitizer (`crush-markup.tsx:175`) — captured for exosphere, not a crush-ast ticket |
| **abi-c** `exo:crates/platform/abi-c` + `include/crush/{capsule.h,capsule_generated.h}` | 719 + 841 headers | capsule-side C ABI; cap entry points return `NotImplemented`; fs/env calls are ambient libc passthroughs; hand-written `capsule.h` declares 17 functions with no implementation | **Drop.** `ast:crates/crush-vm-capi` (embed the VM from C) and `ast:crates/crush-ffi` (plugin ABI) are the sanctioned C surfaces; the ambient calls contradict the cap model |
| **platform/runtimes/*** `exo:crates/platform/runtimes/{python,js,c,go,rust,zig,lua,jvm,swift,ai}` | ~12.5k | per-language execution crates registered as nanovm capabilities | python/js/c/go → **superseded** by `EXEC_LANG` (`ast:crates/crush-vm/src/scheduler.rs`) + buckets (`bucket_exec.rs`) + walkers; rust/zig/jvm/swift/bun → **dead** (never executes / broken / thin unsandboxed subprocess); lua → CRUSH-162 decision; **ai → port engine only** (CRUSH-156–158). One design idea worth a ticket: python worker's guest→host callback bridge protocol (CRUSH-166, design note) |
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
aliases, which only matter if exo-emitted CASM is ever fed to ast's FastVM (folded into CRUSH-163).
Value model: nanovm `RuntimeValue` is byte-identical to `ast:crates/crush-vm/src/value.rs`; CVM1's
`Value` (`vm.rs:128`) is a strict superset of nanovm's heap objects.

**Exosphere's live pins on crush-ast** (for whoever repoints a consumer): only exo-light, via crates.io
`crush-vm = "0.3.6"` (`exo-light/Cargo.toml:36`). exosphere's `Cargo.lock` therefore already holds
both its in-tree `casm 0.1.0` and crates.io `casm 0.3.0` — EXO-194's "rename before both casms meet"
guard has been overtaken (harmless while the versions differ). ~20 exosphere crates still path-dep
nanovm; that is exosphere's to retire, not a crush-ast task.

## 4. squeeze

**State** (`sq:` `master` `f451789`): v0.1.0, never published, no tags, no tests, no CI. One source
file, `src/main.rs` (227 lines). Its own logic is ~60 lines of composition over `crush-pkg`'s public
API: a bare `squeeze` does check → build → write `target/` → run, and `build`/`check` refuse non-Crush
capsules with a clear message (`require_crush_buildable`). Everything else it does, `crush-pkg` already
does — and `crush-pkg` has more (`run` args pass-through, `--message-format text|json|strict` NDJSON,
`pack`/`unpack`, `generate-keys`/`sign`/`verify`, `site`, `show`, `lint`). README/CHANGELOG ("Script/
Native refused"), STATE.md ("M2 unmerged") and RELEASE.md (deleted worktree) are stale; SQUEEZE-5
names a `crush_pkg::ops` module that does not exist (the real modules are `packer`, `signer`, `site`,
`ecap`).

**Dependencies:** `crush-pkg` and `crush-vm`, both `path = "../../crush-ast/crates/…"` + `version =
"0.3.0"`. Every `crush_pkg` item it uses (`Manifest`, `builder::PackageBuilder`,
`runners::{ExecutionResult, get_runner_for_payload}`, `manifest::{manifest_path, scaffold_package}`)
still exists on `ast:` main.

**SQUEEZE-7 still applies:** `crush_vm` is referenced nowhere in `sq:src/` (no features or cfg either);
the dep is dead (tracked with `ast:` CRUSH-86).

**What it needs now that crush-* 0.3.9 is on crates.io** (checked live 2026-10-07: `crush-vm`,
`crush-frontend`, `casm`, `crush-lang-sdk`, `crush-lang-python`, `crush-lang-js` at 0.3.9;
`crush-buckets` 0.1.0; **`crush-pkg` not published**):
1. Publish `crush-pkg` — it is the only blocker. Its deps are all published; nothing in its manifest
   says `publish = false`; it is simply missing from CRUSH-104's publish set. Pre-flight: `cargo
   publish --dry-run -p crush-pkg` (check its `readme` path and the `buckets` dep's `version`). → CRUSH-161.
2. Drop the dead `crush-vm` dep (SQUEEZE-7).
3. Raise the requirement to `crush-pkg = "0.3.9"` (`^0.3.0` would accept an older, API-incompatible
   release if one is ever published).
4. Fix the stale docs, then `cargo package --list` + `--dry-run`.

**Separate repo or fold into crush-pkg? Recommendation: fold.** squeeze's unique behaviour (default
build-then-run, the non-Crush guard) is ~60 lines that belong next to the API they compose; folding
removes a cross-repo publish dependency (squeeze cannot ship before crush-pkg anyway), gives users
crush-pkg's diagnostics and args pass-through, and makes SQUEEZE-5 (re-wrapping pack/sign/verify)
moot. The case for staying separate is brand + a cargo-shaped stable CLI contract on its own release
cadence; if that matters, keep `squeeze` as a binary-only crate that re-exports crush-pkg's CLI under
its name. Either way, steps 1–2 come first. **Captain decision** (it is a public repo with its own
name) → CRUSH-167 is written to work for either outcome.

## 5. Phase-2 plan — dependency order

Tickets are filed as `.jagent/planning/tickets/CRUSH-1NN-*.md` with scope, files, done-condition and a
turn estimate; this section is the ordering. Lanes are independent of each other unless an arrow says
otherwise, so up to five builders can run in parallel — but the box usually carries 1–2 build lanes
(`derby gate`), so the default is **one builder taking lanes in the order A → C → B → D → E**.
🔒 = needs a captain decision first (§5.1). ⚠ = touches a lane-guarded path (`crush-vm/src/fastvm/`).

| Lane | Order | Ticket | Scope | Turns | Runs alone? |
|---|---|---|---|---:|---|
| **A — capabilities** (`crush-lang-sdk`) | 1 | CRUSH-113 (existing) 🔒C-1 | stdlib default-on; `--stdlib` without the feature = hard error; repl parity; fix `stdlib.rs` header | 20 | yes |
| | 2 | CRUSH-151 | fs coreutils host caps `fs.ls/cat/pwd/mkdir/rm/cp/mv/touch/find` under `--fs` sandbox (🔒C-5 for `fs.cd`) | 60 | yes |
| | 3 | CRUSH-152 | `async.sleep` → shared `time.sleep` impl under `--time` | 15 | yes |
| | 4 | CRUSH-153 | `env.all`/`env.home_dir` (env grant); `http.put/delete/request` (`net` feature) | 30 | yes |
| | 5 | CRUSH-154 🔒C-6 | `storage.*` handle-based store caps — or record the decline in favour of `db.*` | 40 | yes |
| | 6 | CRUSH-155 (optional) | `effects` metadata on `HostCapSpec` | 20 | yes |
| **B — AI engine** (`ai_native`) | 1 | CRUSH-156 ⚠ | real argc / arg pass-through for `ai_native.*` (incl. `fastvm::resolve_host_request`) | 40 | yes |
| | 2 | CRUSH-157 | `ai_native.toolchain` strategy engine, each step dispatched through `HostCaps` | 60 | after 156 |
| | 3 | CRUSH-158 | `QueryProvider` / `DelegationBackend` traits + delegation selection | 30 | after 156 |
| **C — debugger** (`crush-debugger`, `PortableVm`) | 1 | CRUSH-159 | step over/out by frame depth, watchpoints; fix stale `todo!()` docs | 70 | yes |
| | 2 | CRUSH-160 | debug event sink, redacted value views, cap-gated `debug.*` scopes | 50 | after 159 |
| **D — packaging** (`crush-pkg`, squeeze) | 1 | CRUSH-161 | make `crush-pkg` publishable (dry-run green, added to the publish lane); **the publish itself is foreman's** | 20 | yes |
| | 2 | CRUSH-167 🔒C-4 | fold squeeze's build-then-run + non-Crush guard + args into `crush-pkg` (or the thin-wrapper variant) | 40 | after 161 |
| | 3 | CRUSH-170 | capability inference: diff caps a package's CAST uses vs `capsule.toml [capabilities]` (`crush-pkg check`) | 60 | after A2–A4 (needs the final cap list) |
| | 4 | CRUSH-171 | manifest category/platform metadata | 25 | yes |
| **E — docs, examples, hygiene** | 1 | CRUSH-168 | stale in-code docs: `casm/src/lib.rs` ecasm comment, `crush-cast/STATUS.md` exo paths, CRUSH-55/EXO-205 pointers | 15 | yes |
| | 2 | CRUSH-169 | planning hygiene: close CRUSH-56/57/88–97/108 as superseded (foreman approves) | 10 | yes |
| | 3 | CRUSH-172 | `examples/crush/capsules/squad-bridge-peek` (scrubbed) | 15 | after 151 |
| | 4 | CRUSH-173 | design notes: import system (vs CRUSH-110), compile pipeline, walker authoring, SBL/corecap layering, WIT note | 40 | yes |
| | 5 | CRUSH-175 | `crush doctor` (python3/node/bash/bwrap/buckets presence + versions) | 25 | yes |
| | 6 | CRUSH-174 | browser playground: check foreman's CRUSH-118 follow-up first; port the old IDE shell only if nothing exists | 20 (+40) | yes |
| **F — gated runtime work** | — | CRUSH-162 🔒C-2 | in-process Lua `EXEC_LANG` (feature `lua`, restricted stdlib, `polyglot.lua` gate) | 50 | yes |
| | — | CRUSH-163 🔒C-3 ⚠ | FastVM yield-servicing host loop + nanovm AI op aliases + watchdog/restart servicing | 70 | yes |
| | — | CRUSH-164 (optional) | wildcard + expiry in `Quotas::allowed_caps` | 15 | yes |
| | — | CRUSH-165 (optional) | CVM1 execution transcript (opcodes + cap calls, hash-chained), feature-gated | 30 | yes |
| | — | CRUSH-166 | design note: guest→host cap callbacks during `EXEC_LANG` (python worker bridge protocol) | 15 | yes |

Total ≈ 885 turns if everything runs (+40 if CRUSH-174 has to port the IDE shell); lanes A + C + D1 + E (the ungated, highest-evidence work) ≈ 450.
CRUSH-176..179 are left unassigned for phase-2/3 fallout.

### 5.1 Decisions needed before tickets are dispatched

| # | Decision | Recommendation | Gates |
|---|---|---|---|
| C-1 | stdlib default-on? | **Yes** — pure, no authority, `regex` already compiled; hard error when absent (§2.4) | CRUSH-113 |
| C-2 | Revive in-process Lua (exosphere's CRUSH-55 W11 verdict was *retire*; adds `mlua`, a C dep) | **No, for now** — no live consumer once exo runtimes retire; re-open with a consumer | CRUSH-162 |
| C-3 | Is FastVM a sanctioned engine that needs a host loop (CRUSH-55 D-4)? | **Defer** — CVM1 is production; decide with CRUSH-77 (four-engine differential) | CRUSH-163 |
| C-4 | squeeze: fold into crush-pkg, or keep as a public repo? | **Fold**, optionally keep `squeeze` as a binary-only re-export (§4) | CRUSH-167 |
| C-5 | `fs.cd`: decline, or a VM-local cwd that `fs.*` resolves against? | **VM-local cwd** inside the `--fs-root` sandbox; never `chdir` the process | CRUSH-151 |
| C-6 | `storage.*` (handle-based) vs existing `db.*` | **Decline** unless a consumer appears; `db.*` covers persistence | CRUSH-154 |
| C-7 | Publish `crush-pkg` to crates.io (irreversible) | **Yes**, after CRUSH-161's dry-run — foreman/captain action | CRUSH-161 → squeeze |
| C-8 | RustPython | **No change.** Nothing in any source needs it; exo's python runtime's rustpython backend is superseded by `EXEC_LANG` + buckets. The ADOPTED "no embedded RustPython VM" decision stands; CRUSH-47 still needs its own decision before work | — |

**Licence/provenance rule for phase 2** (not a decision, a constraint): `crushed-book`, `walker` and
`joker-coordinator` carry no licence, and `incubator` is private — **re-implement from the summaries
here, never copy text or code**. `nixpt/crush` is MIT/Apache like crush-ast, but its code is
dependency-cyclic and an older API, so re-implementation is the practical route anyway. Scrub personal
absolute paths from anything moved (found in `crushed-book`, `joker-*`, `crush-capsules`).

### 5.2 Not crush-ast tickets (handed back to their owners)

- exosphere: `capsule-ui/src/crush/crush-markup.tsx:175` unsanitized HTML injection; two `casm` crates
  in one lockfile (EXO-194 hazard 1); vortex REPL repoint to `crush_lang_sdk::repl`; vortex's
  `shell/bridge.rs` import may break `--no-default-features` (unbuilt); mirror `joker-protocol`'s
  `specs/ACP.md` into `ai-protocols/`.
- crush-vscode: `crush.svg`/`casm.svg` icons from `crush-sdk/assets/icons` (licence check first).
- squeeze: SQUEEZE-7 (dead `crush-vm` dep), stale README/CHANGELOG/STATE/RELEASE, SQUEEZE-5's
  non-existent `crush_pkg::ops` — moot if C-4 = fold.

## 6. Dead weight — do NOT move

| What | Why |
|---|---|
| exo `crush-lang`, `crush-cast`, `casm`, `errors`, nanovm's FastVM/memory/value/registry | ast is a strict superset (CRUSH-55 App. A/B) |
| `ecasm.rs`, nanovm `secure_mem` | encrypted execution: exo-owned at-rest crypto with live exo consumers; never enabled in the VM; no ast consumer (CRUSH-80) |
| nanovm `wasi_bridge`, `bytecode`, `runtime`, `capsule`, `interface`, `codec`, `events`, `ipc`, `RichValue` | no consumer / parallel unused format / always-`None` hooks |
| nanovm `lifecycle`, `pool`, `task` supervision, `phases`; Wave3 kernel; `ScopedHal`/`CapEngine` | belong to antarikshya mandala / x-ray / exo-light (EXO-194 D2); `phases` metrics are fabricated |
| vm-runtime, crush-common (`hal`, `icbf`, `abi`, `lifecycle`, `event_loop`), crush-sdk + `wave3.rs`, abi-c | facades and capsule-packaging types; stub cap handles; ambient libc calls that contradict the cap model |
| exo runtimes rust / zig / jvm / swift / bun, and python's pyo3/rustpython backends | never executes / likely broken / unsandboxed; `EXEC_LANG` + buckets is stronger isolation |
| exo stdlib `polyglot_bridge`, `ai_capabilities` impls, `dom.rs`, `task.rs`, `data.*`, `print`/`text.echo`, `missing_capabilities`/`implementation_plan`/`bin/analysis` | mocks, placeholders, always-error stubs, duplicates, uncompiled scaffolding (§2.2) |
| exo `archive/archived-stdlib`, `tests/stdlib` Rust tests, `exosphere-1.0.zip` restore plan | older snapshot of the already-ported live crate; tests never assert (§2.3, §2.5) |
| `anc:crush` `benchmarks/`, `examples/{types,memory}`, old `.cast`/`.casm` goldens, `bin/crush`, `exo-core`, `mesh`, `exo-metrics`, `nexus-privacy`, `crush-web-dashboard` | simulated benchmarks; syntax never implemented; obsolete format; exosphere/OS scope |
| `crushed-book` chapters `configuration`, `nanovm`, `encrypted_execution`, `stdlib_*`, getting-started/CLI | **overclaims** — describe a config loader, packman/`exo` CLI, trust engine and cap names crush-ast does not have. Porting them would make the public docs less true |
| `walker/*`, `crush-sdk` (all), `joker-coordinator`, `joker-protocol` (except two design notes) | superseded by ast walkers / absorbed by exosphere / agent coordination, not Crush |
| `crush-capsules` except `squad-bridge-peek` and the category metadata idea | path-deps don't resolve; exo SDK capsule model; games already ported; `teddy` moved to its own repo |
| `_rama-archive` | OS capsules, not Crush |
| `incubator` except the shadow-capsule *idea* | private, transcripts, superseded runners/registries |

