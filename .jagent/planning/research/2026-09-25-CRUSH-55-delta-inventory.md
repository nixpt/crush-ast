# CRUSH-55 — exosphere ↔ crush-ast delta inventory

**Date:** 2026-09-25 · **Trees:** `exo:` = nixpt/exosphere @ `8d52996`; `ast:` = crush-ast @ `868be4f`
(`claude/stdlib-nanovm-crush-ast-port-477gv7`, i.e. main `11396b2` + CRUSH-122) · **Method:** read-only
diffs, symbol-set comparisons and consumer greps across both trees; nothing built or run unless a
line says so. Detailed per-area tables are in Appendices A–C; this page is the summary and the
proposed reconcile plan.

Governing decision: **EXO-194 (DECIDED 2026-07-05) — passive convergence.** The exosphere in-tree fork
(`crush-lang`, `crush-cast`, `casm`, `nanovm`, `stdlib`, `vm-runtime`, `platform/runtimes/*`) is frozen,
not migrated in a wave. Dependents inside exosphere at `8d52996`: `nanovm` 22, `casm` 12,
`crush-lang` 6, `crush-cast` 6, `stdlib` 6, `vm-runtime` 3, `crush-common` 52, `crush-errors` 17.
So this inventory answers "what would crush-ast need so a consumer *can* move", not "move everything".

## Headline

The ticket's framing — a *both-ways* feature divergence — is mostly out of date. Across the front end,
IR, assembly and VM, **exosphere's fork is essentially a strict subset of crush-ast**: no exo-only
grammar construct, CAST node, casm opcode, or `FastOp` exists (65 vs 113 casm op names, 0 exo-only;
`RuntimeValue` byte-identical). The three exosphere-only features the ticket names do not hold up:

| Ticket claim | Finding |
|---|---|
| corecaps stdlib | **Done** — CRUSH-122 (PR #61) |
| PolyglotContext sandboxing | exo's `polyglot_imports.rs` is **not in its `lib.rs`** — never compiled; its one extra fn `execute_in_sandbox` returns `"Simulated execution of {lang} code"`. The live polyglot gate (`exec:<lang>` + allowlist) is EQUIVALENT to ast's `polyglot_gate` |
| AI-metadata | `Program.ai_meta` / `AIMetadata` exists on both sides; ast's is the superset |
| Wave3 gating | exosphere's identity/capability kernel (`wave3-kernel`, `can_grant_blocking(did, cap)`), active only when the packman orchestrator attaches a DID. ast has the same shape via `ai_native.*` / `polyglot.<lang>` HostCaps → **belongs in exo-light** (EXO-194 D2), not crush-ast |

What exosphere genuinely has that crush-ast lacks is small and mostly **engine behaviour behind stubs
in crush-ast**, not language surface.

## PORT list (into crush-ast), ranked

| # | What | From | To | Size | Why |
|---|---|---|---|---|---|
| 1 | **AI tool-chain engine + arg plumbing** — `ai_native.*` caps get real argc (today argc 0, VM args dropped); `ai_native.toolchain` runs Sequential/Parallel/Conditional/Retry × FailFast/Continue/Retry/Fallback, each step dispatched through `HostCaps` so grants apply per tool; delegation *selection* + `QueryProvider`/`DelegationBackend` traits | `exo:platform/runtimes/ai/src/{toolchain,delegation}.rs`, `exo:core/vm/nanovm/src/vm/ai.rs::parse_tools` | `ast:crush-lang-sdk/src/ai_native.rs` (+ cap specs; `fastvm::resolve_host_request` arg pass-through is lane-guarded) | M, ~500–700 L | The real CRUSH-32 follow-up. Backends (joker-mcp, foreman-dispatch, `agent-mem`, hard-coded box paths) stay **out** — host-supplied by Antarikshya ai-core |
| 2 | **Debugger semantics** — step over/out by frame depth, watchpoints, event-sink trait, visibility levels (redacted value views) | `exo:core/vm/nanovm/src/debug/` (3.2k), `exo:platform/sdk/vm-runtime/src/vm_debug/` | `ast:crush-debugger` (scaffold, 4 `todo!()`) over `PortableVm::step` | M, ~800–1,200 L | Reimplement, don't copy (exo's is coupled to `Task`/`VmContext`). Gives vortex's debugger users a migration target |
| 3 | `SubprocessWalker::resolve_binary` — PATH + `~/.cargo/bin`, `~/.local/bin` fallback, clean not-found error | `exo:core/crush-lang/src/language_walkers.rs:200` | `ast:crush-frontend/src/language_walkers.rs` | XS, ~45 L | Walkers launched from a daemon/GUI with a thin PATH |
| 4 | *(optional)* wildcard + expiry in `Quotas::allowed_caps` | `exo:vm-runtime/src/enforcement.rs::Scope::matches` | `ast:crush-vm` caps | XS, ~50 L | Only if a consumer needs `fs.*`-style grants |

## Fix in crush-ast (found by the inventory, not ports)

1. **Walker binary-name mismatch — likely broken today.** The registry looks up `crush_lang_go`,
   `crush_lang_zig`, `crush_lang_wasm` (`ast:crush-frontend/src/language_walkers.rs:266,308,344`,
   `ast:cli/src/main.rs:24-28`), but Cargo builds `crush-lang-go`, `crush-lang-zig`,
   `crush-lang-wasm` (`[[bin]] name` / package name). Confirmed from the manifests; not run.
   ~3 L + a test asserting every registered name is a workspace bin.
2. **Orphaned `ast:crush-vm/src/polyglot/` (2.2k L)** — no `mod polyglot` anywhere, only a
   commented-out reference (`fastvm/execution.rs:1214`), and its `mlua`/`boa` deps are absent.
   Delete it (or wire it deliberately — see decision D-3).
3. *(lane-guarded, flag only)* nanovm spells three AI ops `ai_goal_decl`, `ai_knowledge_share`,
   `ai_tool_chain`; ast's FastVM lowerer expects `ai_goal_declaration`, `ai_knowledge_sharing`,
   `ai_toolchain`. Exo-emitted CASM using them would not lower. Fix lives in `crush-vm/src/fastvm/`.

## Stays in / moves within exosphere (prerequisites before any consumer repoints at crush-ast crates)

- **Encrypted CASM (`ecasm.rs`, 1,014 L)** — live: `ant crush encrypt`
  (`exo:exo/cli/src/handlers/encrypt.rs`) and `.ecap` loading (`exo:exo/cli/src/loader_bridge.rs:144-159`).
  crush-ast deleting it (CRUSH-80) was correct; it must move to an exo-owned crate (e.g. `exo-ecasm`,
  generic over `casm::Program`) **before** exo swaps to ast's `casm`.
- **`CachedProgram`** — nanovm's lookup cache (`exo:nanovm/src/vm/context.rs`); a VM concern.
- **`crush-errors::ResultExt`** — 0 impls, 0 callers, but re-exported by `crush-common` and `exo/core`;
  drop the two re-exports when swapping.
- **Rename guard (EXO-194 rule 3)** — the two `casm` crates still share a name. Still no build graph
  pulls both; rename before one does.

## Out of scope → elsewhere

`lifecycle::ResourceStats`, `pool::SymbolCache` → antarikshya mandala (W12) · `phases` → x-ray (W12;
note its metrics are fabricated — `granted: true` hard-coded — recommend x-ray decline) ·
`ScopedHal`/`CapEngine`, Wave3 kernel → exo-light (EXO-194 D2) · crush-common `abi`/`capability`/
`lifecycle` → capsule-contract · `event_loop` → next to its one user (`exo-hal`) · Python `dom.rs` →
surfer/arniko.

## Decline (dead, stub, or no consumer)

nanovm: `secure_mem` (never enabled), `wasi_bridge`, `bytecode` (parallel unused format), `runtime`
(`native_runtime` always `None`), `audit` (`record_cap_call` never called), `events`, `capsule`,
`interface`, `codec`, `ipc`, `RichValue`, `task` supervision (no consumer beyond a display),
effect-permission enforcement (never called). vm-runtime: `CapabilityHandle::call` (TODO → `Null`),
`hal`, `net`, `macros`, `vm.rs_stub`. crush-common: `icbf` (superseded by `HostCapSpec`),
`SeahorseHalAi` (dead). crush-sdk / `wave3.rs`, abi-c (cap entry points `NotImplemented`, ambient fs/env).

## W11 — runtimes (input for the decision; not crush-ast's to make)

| Runtime | Verdict | Reason |
|---|---|---|
| python, js | EQUIVALENT → retire exo copy | ast `EXEC_LANG` + buckets (CRUSH-20/66) is stronger isolation than a module blocklist / boa |
| c, go | EQUIVALENT → retire | exo "transpile" mode only compiles, never runs; ast walkers + VM do it for real. Native modes (in-process dlopen, `go run`) break the cap model |
| ai | **PORT engine** (PORT #1) | backends → Antarikshya ai-core |
| rust | RETIRE | `cargo build`s then returns placeholders; never runs the binary |
| zig | RETIRE | wasmtime linker has no WASI imports — likely broken (unverified) |
| lua | RETIRE *or* PORT-lite (D-3) | exo's "Sandbox" mode is a no-op; nanovm's polyglot Lua has a restricted stdlib |
| jvm, swift, bun | RETIRE | thin unsandboxed subprocesses; re-add as a buckets lang entry if ever wanted |

Suggested hosting rule: Antarikshya hosts exactly the `EXEC_LANG` set (python, node, bash) via buckets;
each new language is a lang→bucket-spec entry, not a runtime crate.

## Corrections to prior docs

- **EXO-194** "CVM1 has no language compiler" — stale: `ast:crush-lang-sdk/src/compile.rs:283`
  `casm_to_vm` + `compile_crush_source` take Crush source to CVM1 inside crush-ast.
- **EXO-205** (`exo:docs/crush/EXO-205-divergence-inventory.md`) — "crush-ast has the ecasm format" (deleted,
  CRUSH-80); "`polyglot/` byte-identical" (only nanovm has JS console capture, and ast's copy isn't compiled).
- **EXTRACTION-PLAYBOOK W9** — says `core/base/common` holds time/which/atomicfs/env/paths/proc
  helpers; it doesn't (those are in `exo/utils`).
- **CRUSH-55 ticket** — the three "exosphere-only" features (table above).

## Proposed reconcile plan (for the exosphere lane owner — nothing lands cross-tree unilaterally)

Slices, each independently shippable, crush-ast side first (no exosphere change required):

1. **ast hygiene** — walker binary-name fix + test; delete orphaned `crush-vm/src/polyglot/`. (S)
2. **CRUSH-32b** — PORT #1 (AI engine + arg plumbing). (M)
3. **Debugger** — PORT #2, as its own ticket. (M)
4. **Walker PATH fallback** — PORT #3. (XS)
5. *(exosphere side, lane owner's call)* move `ecasm` + `CachedProgram` to exo-owned crates; this is
   the only prerequisite for any exo crate to repoint onto ast's `casm`.

### Decisions needed

- **D-1 (exo lane owner):** accept that CRUSH-55's "both-ways divergence" is resolved as "ast ⊇ exo",
  and close CRUSH-55 after slices 1–4 are ticketed?
- **D-2 (captain):** W11 runtime verdicts above.
- **D-3:** Lua — port an in-process restricted-stdlib Lua `EXEC_LANG` (feature-gated, ~200 L + `mlua`),
  or retire Lua entirely? No live consumer needs it once exo's runtimes retire.
- **D-4:** the lane-guarded FastVM items (AI op aliases; FastVM has no yield-servicing host loop —
  `CallHost`/`ExecLang`/`Spawn`/`Await` aren't serviced, which only matters if FastVM stays a
  sanctioned engine next to CVM1).

---

## Appendix A — front end, CAST, casm, errors


This covers exo@8d52996 (`crates/core/{crush-lang,crush-cast,vm/casm,base/errors}`) against the crush-ast working tree (branch `claude/stdlib-nanovm-crush-ast-port-477gv7`, HEAD 868be4f). It was read-only: nothing was built or run. Method: `diff -u` of each shared file, then comparisons of the `fn`, token, keyword, opcode-string and enum-variant sets, then greps for consumers across all of exosphere (`/target/` excluded).

**Headline:** in all four crates, exosphere is essentially a strict *subset* of crush-ast. Most of each "exo is bigger" file-size gap comes from doc/inline comments, `async fn` wrappers and `uuid`/`url`/`tokio` usage. None of that is new behaviour. Exosphere has only three things of its own with real behaviour: encrypted CASM (`ecasm.rs`), which has live consumers; `CachedProgram`, which nanovm uses; and walker-binary PATH fallback. The ticket says exosphere has "PolyglotContext sandboxing" and "AI-metadata". Both are wrong for this scope:
- **PolyglotContext:** exosphere's `polyglot_imports.rs` is an **orphan file**. It is not in exosphere's `lib.rs` module tree, so it is never compiled. crush-ast compiles it (`ast:crates/crush-frontend/src/lib.rs:11`).
- **AI-metadata:** `AIMetadata`/`ai_meta` exist on both sides, and crush-ast's version is the superset.
- **"Wave3 gating":** nothing in these four crates. The only mention is doc text in `errors/src/version.rs`, which is identical on both sides. The live Wave3 code is in `exo:crates/platform/sdk/crush-sdk/src/wave3.rs` and `wave3-sdk`, which are outside scope A.

### 1. `exo:crates/core/crush-lang` vs `ast:crates/crush-frontend` (+ `crush-lang-sdk` REPL, `crush-debugger`)

Function-name sets: parser/mod.rs, lexer.rs, compiler.rs, semantics.rs, optimizer.rs, render.rs and types.rs have **zero exo-only `fn` names**. crush-ast has +30, +10, +6, +4, +1 and +26 extra in those files respectively; types.rs adds 4 variants. The lexer has no exo-only tokens or keywords. The parser, semantics, optimizer and render files have no exo-only string literals. The only exo-only literals in the compiler are raw op strings (`"mul"`, `"pop"`, …). crush-ast emits the same ops through typed `Instruction::from_opcode`.

| Feature | exosphere | crush-ast | Verdict | Note |
|---|---|---|---|---|
| Lexer / keywords | exo:crates/core/crush-lang/src/parser/lexer.rs:318 | ast:crates/crush-frontend/src/parser/lexer.rs:437 | CRUSH-AST AHEAD | ast adds `new`/`FatArrow` tokens and multilingual keyword aliases (`natra`, `否则`, `それ以外`, …). Exo has no token of its own. |
| Parser grammar | exo:…/parser/mod.rs (1991 L) | ast:…/parser/mod.rs (3138 L) + parser/cson.rs | CRUSH-AST AHEAD | No exo-only grammar construct found. ast adds annotations (`@module`/`@invariant`/`@errors`), CSON, tuple/list/vector/set literals, `Assign`, `VectorMath`, `is_async`. |
| `ast.rs` | exo:…/src/ast.rs:2 (`pub use crush_cast::*`) | — (uses `crush_cast` directly) | EQUIVALENT | Just a re-export shim. |
| `types.rs` | exo:…/src/types.rs (63 L) | ast:crates/crush-frontend/src/types.rs:15 | CRUSH-AST AHEAD | ast adds `Tuple/List/Vector/Set`. |
| semantics / optimizer / compiler | exo:…/semantics.rs, optimizer.rs, compiler.rs:1218 (AI lowering) | ast:…/semantics.rs, optimizer.rs, compiler.rs:2027 | CRUSH-AST AHEAD | Same structure. ast also lowers `SemanticMatch`/`Synthesize` (compiler.rs:2242, 2257), try/`enter_try`, and collections. |
| Analysis passes | — | ast:…/cast_enrich.rs, exhaustive_check.rs, mutation_check.rs, wip_check.rs, diagnostics.rs, `check_source` in lib.rs | CRUSH-AST AHEAD | Headline ast-only features. |
| `import_system.rs` (846 vs 731) | exo:…/import_system.rs | ast:…/import_system.rs | EQUIVALENT | The whole delta is doc comments, `async fn` (no `.await` on real I/O inside, so async buys nothing), `uuid` handles vs ast's `AtomicU64` counter, and `url::Url::parse` vs ast's manual `https://` strip. ast is *stricter* here: it requires `https://`. Exo's `Url::parse` accepts `http://`, but the trusted-domain check still applies. Secure-env, MCP, capability and external imports: same variants on both sides. No exo-only function. |
| `polyglot_imports.rs` / `PolyglotContext` (689 vs 543) | exo:…/polyglot_imports.rs:12 (**not in exo lib.rs, so never compiled**) | ast:…/polyglot_imports.rs:25 (compiled) | DECLINE | Exo-only extras: `execute_in_sandbox` (exo:…:271). It is a **stub**: it returns `output: "Simulated execution of {lang} code"` with a hard-coded 0.1 s and 1024 B. There is also an `ExecutionResult` struct and 2 tokio tests. The file has zero consumers and does not build in exo. crush-ast already has the real resolve/transform/implicit-scan logic. |
| `language_walkers.rs` (593 vs 482): `SubprocessWalker::resolve_binary` | exo:…/language_walkers.rs:200 | — (ast:…/language_walkers.rs:139 calls `Command::new(name)` directly) | PORT (low) | Checks the name as a path, then walks `PATH`, then tries `~/.cargo/bin`, `~/.local/bin`, `/usr/local/bin` and `/usr/bin`, and returns a clean `WalkerError` if it finds nothing. Useful when launched from a daemon or GUI with a thin PATH. Goes in ast `language_walkers.rs`, about 45 L including 2 tests. |
| Walker registry binary names | exo:…:239ff (`ts_walker`, `go_walker`, `wasm_walker`, `cpp`) | ast:…:174ff (`crush_lang_go`, `crush_lang_zig`, `crush_lang_wasm`, `c_cpp`) | DECLINE (exo names) + **ast bug** | Exo's names don't match any real binary either. But ast's `crush_lang_{go,zig,wasm}` don't match the Cargo bin names `crush-lang-go`, `crush-lang-zig` and `crush-lang-wasm` (default bin from `src/main.rs`) either: underscore vs hyphen. ast's registry also adds Zig. Whether this breaks at runtime is unverified (not run), but by Cargo naming rules it looks broken. |
| async `walk_to_cast` / `auto_walk_to_cast` | exo:…/language_walkers.rs | ast (sync) | EQUIVALENT | Exo's are `async` wrappers around blocking `std::process::Command`. The only consumer is `exo:tests/e2e/full_pipeline_test.rs:372,557`. |
| `ai_runtime.rs` (277 vs 240) | exo:…/ai_runtime.rs:147-240 | ast:…/ai_runtime.rs | EQUIVALENT | **Code is identical**: with comments and blank lines stripped, the diff is empty. The 37-line gap is doc comments. Neither side has a consumer outside the file. (`exo:crates/core/base/stdlib` uses a different crate, `crush_ai_runtime`.) |
| `render.rs` (`render_program`) | exo:…/render.rs (1104 L) | ast:…/render.rs (1547 L) | CRUSH-AST AHEAD | No exo-only fns. Exo consumer: `exo:crates/exo/cli/src/handlers/render.rs:10`. |
| Source REPL (`repl::run`) | exo:…/repl.rs:298 (nanovm + `DummyHal`) | ast:crates/crush-lang-sdk/src/repl.rs:541 (+ repl_helper.rs, repl_util.rs) | CRUSH-AST AHEAD | ast's REPL descends from exo's: same `merge_snippet`, `merge_main_statements` and `is_input_complete`. It adds quotas, `--stdlib` and themed diagnostics. `crush-debugger` is a different tool: a CVM1-assembly step debugger, not a source REPL. Exo consumer: `exo:crates/exo/vortex/src/crush/mod.rs:9`. |
| Bins `crush-compiler` (CAST JSON → .casm/.casmb), `crush-run`, `crush-repl`, `crush-to-cast` | exo:…/src/bin/*.rs | ast:crates/crush-lang-sdk/src/bin/{crushc,crush-compile,crush-run,crush-repl,crush}.rs | EQUIVALENT / AHEAD | Exo's run bins are wired to nanovm. ast's equivalents are richer (crushc 334 L, crush-run 396 L). Exo's `crush-compiler.rs` isn't declared in exo Cargo.toml `[[bin]]`, but Cargo's bin auto-discovery should still pick it up (not built to confirm). |
| Public API used by exo consumers | `parse_source`, `compile_source`, `compile_cast`, `Compiler::new`, `ast::*`, `render_program`, `repl::run` | ast:crates/crush-frontend/src/lib.rs (`parse_source`, `compile_cast`, `compile_cast_owned`, `check_source`) | EQUIVALENT | This is the surface the 9 exo dependents actually touch. ast has all of it except `compile_source`/`compile` (thin wrappers) and `repl` (it moved to crush-lang-sdk). |

### 2. `exo:crates/core/crush-cast` (1.0.0) vs `ast:crates/crush-cast`

There is **no exo-only node, field or variant.** Across lib.rs, ai.rs, types.rs, validate.rs, pack.rs, diff.rs and format.rs, the only `+` lines on the exo side are older derive lines (no `Default`) and rustfmt differences. `export-py` has no exo-only fn.

| Feature | exosphere | crush-ast | Verdict | Note |
|---|---|---|---|---|
| `Program.ai_meta: Option<AIMetadata>` | exo:crates/core/crush-cast/src/lib.rs:23, ai.rs:283 | ast:crates/crush-cast/src/lib.rs:36, ai.rs:314 | EQUIVALENT | "AI-metadata" is **not** exo-only. |
| `AIExpression` variants | exo:…/ai.rs:10 | ast:…/ai.rs:10 (+`SemanticMatch`:60, `Synthesize`, `SemanticSwitch`) | CRUSH-AST AHEAD | |
| Module manifest / annotations | — | ast:crates/crush-cast/src/manifest.rs:25,214 (`ModuleManifest`, `FunctionAnnotations`, `ExhaustiveMatchSite`, `WipNode`, `TemporaryNode`, `DecisionNode`) | CRUSH-AST AHEAD | These are new `Program`/`Function` fields. All `serde(default)`, so exo-produced CAST JSON still loads in ast. |
| `CastType` | exo:…/types.rs | ast:…/types.rs (+`F32`, `BigInt`, `Complex`, `Tensor`, `Tuple`, `List`, `Vector`, `Set`) | CRUSH-AST AHEAD | |
| Statements/exprs | exo:…/lib.rs | ast:…/lib.rs (+`Assign`, `VectorMath`, `TupleLiteral`, `ListLiteral`, `VectorLiteral`, `SetLiteral`, `deps`, `is_async`) | CRUSH-AST AHEAD | |
| CSON | — | ast:…/cson.rs | CRUSH-AST AHEAD | |
| pack / diff / format / validate | identical modulo fmt | same | EQUIVALENT | |
| `export-py` (1251 vs 2721 L), `export-ts` | exo:…/src/bin | ast:…/src/bin | CRUSH-AST AHEAD / EQUIVALENT | |

### 3. `exo:crates/core/vm/casm` vs `ast:crates/casm`

Opcode string names: exo has 65, ast has 113, and **0 are exo-only**. `Program`/`Function`/`Manifest`/`Format` have the same shape. `CASM_VERSION = "1.0"` and the major-version gate are the same, and so are `.casm` (JSON) and `.casmb` (MessagePack via `Format::from_path`).

| Feature | exosphere | crush-ast | Verdict | Note |
|---|---|---|---|---|
| OpCode set | exo:crates/core/vm/casm/src/lib.rs:68 | ast:crates/casm/src/lib.rs:66 | CRUSH-AST AHEAD | ast adds `EnterTry`/`ExitTry`/`Throw`, `Index`/`Len`/`ArrayPush`/`ArrayPop`/`MakeRange`, tuple/list/vector/set ops, 13 `Ai*` ops, `Math*`/`Str*`, `Dom*` and `Halt`. |
| `.casm` JSON / `.casmb` MsgPack | exo:…/lib.rs:426-436, 601, 621 | ast:…/lib.rs:557-567, 664, 684 | EQUIVALENT | |
| `Function.type_hints` | — | ast:…/lib.rs:253 | CRUSH-AST AHEAD | |
| `Instruction::from_opcode` (typed emission) | — | ast:…/lib.rs:286 | CRUSH-AST AHEAD | |
| `DebugInfo.fn_offsets` + `source_location_for_function_pc` | — | ast:crates/casm/src/debug_info.rs:117 | CRUSH-AST AHEAD | |
| `CachedInstruction` / `CachedProgram` / `CachedFunction` / `Program::to_cached` | exo:…/lib.rs:213, 496 | — (deleted in 1cd2506) | DECLINE for ast | Live in exo: `exo:crates/core/vm/nanovm/src/vm/context.rs:5,64,83` (nanovm fast function lookup). It is an interpreter-side cache, so it belongs in the VM, not the IR crate. ast's FastVM does its own pre-decoding. Note a quirk: `call_targets` indexes `HashMap::keys()` order, which isn't stable across runs. |
| Encrypted CASM `.ecasm` (`EcasmFile`, ChaCha20-Poly1305 per-page, magic `ECSM`, 92 B header, page table, capsule-id and caps-hash binding) | exo:crates/core/vm/casm/src/ecasm.rs:54-609 (1014 L). Its inline tests don't compile (EXO-151, noted at lib.rs:673). | — (deleted: ast 1cd2506, CRUSH-80, "dead code") | DECLINE for ast (keep in exo) | **Exosphere still consumes it.** `ant crush encrypt` goes through `exo:crates/exo/cli/src/handlers/crush.rs:38` into `handlers/encrypt.rs:2,63`. `.ecap` loading, used by `ant run`/`monitor`, goes through `exo:crates/exo/cli/src/loader_bridge.rs:144-159` (`EcasmFile::deserialize` + `decrypt`, with keys from `exo-crypto`). nanovm `secure_mem` references the ECASM page layout. CRUSH-80 was right for crush-ast, which has no consumer. But if exo ever repoints at crush-ast's `casm`, `ecasm.rs` must first move to an exo-owned crate (e.g. `exo-ecasm`, generic over `casm::Program`). It is an exo packaging/at-rest-crypto concern and doesn't belong in the toolchain. |
| Error mapping on deserialize | exo:…/lib.rs (inline `map_err`) | ast (fmt-only diff) | EQUIVALENT | |

### 4. `exo:crates/core/base/errors` vs `ast:crates/crush-errors`

Nearly identical: 937 L each. convert.rs and kinds.rs are byte-identical. version.rs differs by fmt only.

| Feature | exosphere | crush-ast | Verdict | Note |
|---|---|---|---|---|
| `trait ResultExt<T,E> { fn into_crush(self) }` | exo:crates/core/base/errors/src/context.rs:91, lib.rs:6 | — | DECLINE (keep a shim if swapping) | It has **no impls and 0 callers** of `into_crush`. But it is re-exported by `exo:crates/core/base/common/src/lib.rs:184` and `exo:crates/exo/core/src/lib.rs:217`, so a straight swap to ast's crush-errors breaks those re-exports. Either drop the re-exports, which is trivial, or add the 4-line trait to ast. |
| Everything else | — | — | EQUIVALENT | |

### Other notes

- **Who uses exosphere's crush-lang:** the dependents are the root workspace, `khukuri-exo`, `exo/runtime-core`, `exo/cli`, `exo/vortex`, `exo/packman`, `capabilities/process-spawn`, `vm/nanovm`, `base/stdlib` and `tests/`. Between them they use only parse/compile/`ast::*`/render/repl, plus `WalkerRegistry` in one e2e test. Nothing outside the file uses `ai_runtime`, `import_system` or `polyglot_imports`.
- **Dependencies:** exosphere's crush-lang depends on `nanovm` for the REPL and run bins, plus `tokio` (full), `uuid` and `url`. crush-ast's front end dropped all four. Porting any of the "async" versions would bring them back for no behavioural gain.

### Top PORT candidates (ranked by value)

1. **Fix crush-ast's own walker registry binary names** (`crush_lang_go`/`_zig`/`_wasm` → `crush-lang-go`/`-zig`/`-wasm`). This isn't a port, but the diff surfaced it, and it's the highest-value item here. About 3 L plus a test that asserts each registered binary name matches a workspace `[[bin]]`/package name (about 20 L). Runtime impact unverified.
2. **`SubprocessWalker::resolve_binary` PATH + well-known-dir fallback** → `ast:crates/crush-frontend/src/language_walkers.rs`. About 40 L of code plus 2 tests (about 15 L). Low value: `Command::new` already searches `PATH`. The gain is `~/.cargo/bin` and `~/.local/bin` when launched with a minimal environment, plus a clear "not found" error.
3. **(Conditional, only if exo swaps to ast's crush-errors)** the `ResultExt` trait: 4 L. Removing exo's 2 re-exports is the better option.

Nothing else in scope A qualifies for PORT. Encrypted CASM (about 1014 L) and `CachedProgram` (about 100 L) are live but exo-owned. They should move to exo-side crates *before* any repoint onto crush-ast's `casm`, not into crush-ast. `execute_in_sandbox` is a stub in a file that doesn't compile.

---

## Appendix B — nanovm vs crush-vm


**Trees:** `exo:` = `/home/user/exosphere` @ `8d52996` (`crates/core/vm/nanovm`). `ast:` = `/home/user/crush-ast` working tree (HEAD `868be4f`), mainly `crates/crush-vm`, plus `crush-debugger`, `crush-jit` and `crush-lang-sdk`.
**Method:** read-only. I read `exo:nanovm/src/lib.rs` for module declarations; every module is compiled unconditionally, and nanovm has no `[features]`. I grepped all of `exo:crates/**` and `exo:tests/**` for `nanovm::` usage, and diffed shared files between the trees.
**Size:** nanovm is 26.2k lines of `src` plus 8.0k of `tests/`+`examples/`, which gives the "34k" figure. Its examples are bitrotted and `autoexamples = false` (`exo:nanovm/Cargo.toml:7-11`).

**Prior art:** `exo:docs/crush/EXO-205-divergence-inventory.md` (2026-08-08) already covers much of this, in §4-§6. This file checks it again and corrects it where it has gone stale:
- **Stale 1.** EXO-205 says "crush-ast has the `ecasm` FORMAT (byte-identical)". It no longer does. `ast:crates/casm/src/` holds only `lib.rs` and `debug_info.rs`, and a comment at `ast:casm/src/lib.rs:772` mentions the removed `ecasm.rs`.
- **Stale 2.** EXO-205 says `polyglot/` is "byte-identical". `builtin_executors.rs` differs: only nanovm has JS `console.log`/`console.error` capture (`exo:nanovm/src/polyglot/builtin_executors.rs:308-345`). More important, **`ast:crush-vm/src/polyglot/` is never compiled.** `ast:crush-vm/src/lib.rs` has no `mod polyglot`, and nothing uses `#[path]` or `crush_vm::polyglot`. It is an orphaned copy.
- **Stale 3.** EXO-194 says "CVM1 has no language compiler". crush-ast now has `casm_to_vm` (`ast:crush-lang-sdk/src/compile.rs:283`) and `compile_crush_source` (`:29`), so crush source reaches CVM1 inside crush-ast.
- **Hazard.** crush-ast's FastVM path, `run_fastvm_with_caps` (`ast:crush-vm/src/vm.rs:~750-770`), runs once with a `DummyHal` and returns the first yield. Nothing services `CallHost`, `ExecLang`, `Spawn` or `Await`. The only servicing is the AI/DOM stub resolver `resolve_host_request` (`ast:fastvm/mod.rs:215-260`). nanovm's `VM` is a full FastVM host driver (`exo:nanovm/src/vm/mod.rs:932-1080`). This matters only if FastVM stays a sanctioned engine. CVM1 (`scheduler.rs`) is crush-ast's production path.

### Module table

Consumer lists exclude `platform/sdk/vm-runtime`, which re-exports almost every module (`vm-runtime/src/lib.rs`) but whose own users only pull `VM`/`VmError`/`VmResult`/`VmState` (`capsule-sdk/src/lib.rs:49`). `khukuri-exo` is commented out of workspace `members` (`exo:Cargo.toml:193-195`).

| Module | lines | real/stub | exo consumers | crush-ast counterpart | Verdict | Note |
|---|---|---|---|---|---|---|
| `vm` (mod, interpreter, ai, context…) | 4636 | real. `call_host`/`call_interface` are placeholders that pop args and push Null (`interpreter.rs:1399-1440`) | runtime-core, cli, vortex, crush-lang, stdlib tests, `tests/` | `ast:vm.rs` + `scheduler.rs` (CVM1 prod) + `portable_vm.rs` + `fastvm/` | EQUIVALENT (core). Parts are PORT / OUT OF SCOPE, see (a)(b)(f) | Interpreter dispatches on string op names (`instr.op.as_str()`, `interpreter.rs:264`). `vm/ai.rs` is real but fleet-coupled: it spawns `joker-mcp` (`ai.rs:16`) and `agent-mem` (`:522`) |
| `debug` | 3166 | real | vortex shell (step into/over/out, breakpoints, watch: `exo:exo/vortex/src/shell/mod.rs`) | `ast:crush-debugger` is a SCAFFOLD with 4 `todo!()` (`lib.rs:3-40`). `portable_vm.rs:233-279` has `set_breakpoints` and `DebugBreak` only | **PORT** (partial) | See (e) |
| `fastvm` | 2450 | real. `arr_set` lowers to `Nop` (`instructions.rs:701`) | khukuri-exo (excluded), via `VM` | `ast:crush-vm/src/fastvm/` (3.8k) | CRUSH-AST AHEAD | ast adds `arithmetic.rs`, collection/math/str/AI ops, a real `ArrSet`, sorted fn order. nanovm fastvm has nothing ast lacks |
| `polyglot` | 2282 | real (Lua via mlua, JS via quick-js, pluggable `RuntimeExecutor` registry). `exec.rs:87,129` placeholders | runtimes c/go/js/lua/python/rust/zig (`register_executor`), `tests/e2e` | orphaned uncompiled copy at `ast:crush-vm/src/polyglot/`. Live path is subprocess `EXEC_LANG` (`scheduler.rs:61-79`, python/js/bash) + `bucket_exec.rs` (bwrap) | **PORT** (Lua in-process only) + ast cleanup | See (d) |
| `phases` | 1685 | **fabricated metrics** (`phase2.rs:629-645` hardcodes `granted: true` and constant durations) | none | none | OUT OF SCOPE → antarikshya `x-ray` (W12) | Recommend x-ray also decline: the metrics are not measurements |
| `value` | 1326 | `RuntimeValue` real. `RichValue`/`Type` (`types.rs:232-336`) is unused and full of TODOs | everyone (`RuntimeValue`) | `ast:crush-vm/src/value.rs` is **identical** to `exo:value/runtime_value.rs` (diff empty) | EQUIVALENT. `RichValue` DECLINE (dead) | See (a) |
| `wasi_bridge` | 1140 | real CBV codec (`serialize.rs`/`deserialize.rs`) | **none** (only the vm-runtime re-export) | none. `CbvError` survives orphaned in `ast:crush-errors/src/convert.rs:125` | DECLINE | No consumer. Not on the VM path |
| `memory` | 1136 | real (arena, GC mark, borrow state) | stdlib, all runtimes, cli | `ast:crush-vm/src/memory.rs`: same arena (21-line diff) + `Tuple/List/Vector/Set` objects | CRUSH-AST AHEAD | exo `InterfaceHandle(crush_common::ObjectHandle)` vs ast `InterfaceHandle(u64)` |
| `secure_mem` | 1083 | real (ChaCha20-Poly1305 paged decrypt + LRU instr cache) but **never enabled**: `VM::enable_encrypted_execution` (`vm/mod.rs:604`) has no caller in exo | none. ECASM is decrypted whole, up front, in `exo:exo/cli/src/loader_bridge.rs:146-156` | none | DECLINE | Dead in practice. Revisit only if confidential execution becomes a CVM1 requirement (EXO-205 "B3" overstated its liveness) |
| `lifecycle` | 975 | real (`LifecycleManager`/`ManagedVm` over FastVM, `ResourceStats`) | khukuri-exo only (excluded from workspace) | none | OUT OF SCOPE → antarikshya `mandala` (quota stats, W12) | The rest of the module has no live consumer: decline |
| `pool` | 956 | real (`VmPool`, `SymbolCache` at `manager.rs:65`) | `task::scheduler` internally | none | OUT OF SCOPE → antarikshya `mandala` `WarmPool` (W12) | |
| `task` | 896 | real (`TaskManager`, `SupervisorPolicy` `manager.rs:57`, `WatchdogTimer` `:111`, `restart_task` `:284`, multi-VM `Scheduler`) | vortex (`TaskState` display), vm-runtime `supervision_tests.rs` | `ast:scheduler.rs` green threads (`SPAWN/AWAIT/YIELD`). ast FastVM lowers `watchdog`/`restart` but nothing services them | DECLINE (for now) | See (e) |
| `bytecode` | 821 | a separate class/method bytecode format + builder, **not used by the VM** | none (re-export only) | CVM1 `ast:bytecode.rs` + `assembler.rs` | DECLINE | Dead parallel format |
| `runtime` | 744 | code is real (mlua/quick-js embed, bwrap `run_sandboxed` `native.rs:326`) but **dead in VM**: `context.native_runtime` is only ever `None` (`vm/mod.rs:357`) | none live | `ast:bucket_exec.rs` (bwrap via `buckets`, `sandboxed-polyglot` feature) | EQUIVALENT (ast's is the live one) | |
| `traits` | 524 | real (`Capability{ic_id,name,allows_reentrancy,effects,call}`, `CasmCapability`, deprecated `HostAdapter`) | stdlib, corecaps, packman, registry, vortex, all runtimes | `ast:host.rs` `HostCap` (CVM1) + `ast:fastvm/mod.rs:19-24` `Capability`/`Hal` | EQUIVALENT (different shape) | See (c) |
| `registry` | 513 | real (`Registry`/`Namespace`, ICBF-id lookup, `register_program`) | corecaps, stdlib, packman, vortex, cli, runtime-core | `ast:host.rs` `HostCaps` + `ast:caps.rs` + `crush-lang-sdk/src/host_caps.rs` | EQUIVALENT | |
| `codec` | 422 | real (JSON/MsgPack `LoweredProgram` codec, byte-order helpers) | stdlib `binary.rs` (byte helpers), khukuri-exo (`load_file`) | none. ast serializes `casm::Program` directly | DECLINE | `LoweredProgram` persistence has no ast consumer. The byte helpers belong with stdlib/corecaps (part C) |
| `capsule` | 399 | thin (`Capsule` trait impl'd for `VM`, `init` → Ok) | none | exo-light `CapsuleRuntime` (outside both trees; unverified here) | DECLINE | |
| `interface` | 289 | thin (impl `core-interfaces` `Component/HealthCheck/VMInterface` for `VM`) | none found (runtime-core mentions `VMInterface` only in docs) | none | DECLINE | |
| `audit` | 288 | partial: `record_instruction` is called (`interpreter.rs:261`), **`record_cap_call` never is**, and FastVM runs bypass it | exo cli `run --audit` (`exo:exo/cli/src/handlers/run.rs:86-93`) | none | DECLINE as-is. Optional redesign in CVM1 | See Top PORT #4 |
| `ai_optimizer` | 124 | real (ONNX GC model) | via `VmContext` | `ast:crush-vm/src/ai_optimizer/mod.rs` (only the model path differs) | EQUIVALENT | |
| `events` | 69 | trivial `EventRegistry` | none | none | DECLINE | |
| `ipc` | 33 | `IpcRequest/IpcResponse` enums | exo cli `run.rs:3,142,169` | none | DECLINE (belongs to exo cli/daemon) | Kernel↔capsule wire, not a VM concern |
| `lib.rs` + `sbl_core.{crush,casm}` | 221 | re-exports / SBL | exo cli `register_sbl_into_registry` | `ast:crush-lang-sdk/src/sbl.rs` | **DONE** (SBL) | |

### (a) Value model

- **Scalar/stack value.** nanovm `RuntimeValue {Int, Float, Bool, Null, Ref(usize), String}` (`exo:nanovm/src/value/runtime_value.rs:7-14`) is **byte-identical** to `ast:crush-vm/src/value.rs`, which is FastVM's value type. So the FastVM tier has not diverged at the value level.
- **Heap objects.** nanovm's `Object` has `Str, Array, Map, Object{lang,class_name,fields}, Tagged, Handle, Bytes, Buffer, Result, InterfaceHandle(ObjectHandle)` (`exo:memory/arena.rs:254-276`). ast adds `Tuple, List, Vector, Set` and uses `InterfaceHandle(u64)` (`ast:memory.rs:254-280`). **CRUSH-AST AHEAD.**
- **CVM1 value.** ast `vm::Value` has 15 variants: `Null, Bool, Int, Float, Str, Array(Rc<RefCell>), Tuple, List, Vector, Set, Map, Error, Bytes, Handle, Foreign` (`ast:vm.rs:124-150`). It is a different, richer, inline (not arena) model, with cross-type numeric equality (`:152+`). nanovm has no counterpart. **CRUSH-AST AHEAD.**
- **nanovm `RichValue`/`Type`** (`value/types.rs:232-336`: Char, Class, Function, Capability, Undefined…) is referenced nowhere outside that file and is TODO-laden (`:308,430,461,558,648`). **DECLINE.**

### (b) Opcode sets

The three executors dispatch differently:
- **nanovm interpreter:** string op names, about 95 ops (`exo:vm/interpreter.rs`).
- **FastVM (both trees):** lowers `casm::Program` op strings to `FastOp`.
- **ast CVM1:** about 100 `u8` opcodes (`ast:bytecode.rs`).

Counts: exo casm `OpCode` has 66 variants and ast casm has 110. nanovm `FastOp` has 78 and ast `FastOp` has 108. **Nothing is exo-only in either enum**; every exo variant name also exists in ast.

- **ast-only (FastOp/casm):**
  - collections: `NewTuple/TuplePush/NewList/ListPush/NewVector/VectorPush/NewSet/SetPush`
  - math: `MathPow/Sqrt/Abs/Round/Floor/Ceil`
  - strings: `StrStartsWith/EndsWith/ToUpper/ToLower/Trim`
  - a real `ArrSet`, and 10 `Ai*` FastOps
  - casm adds `Halt`, `Throw`, `EnterTry`/`ExitTry`, `Index`, `Len`, `MakeRange` and DOM ops
  - CVM1 additionally has `DOM_*` (10), `VEC_ADD/VEC_DOT/MAT_MUL`, `PRINT` and `HALT`
- **nanovm-interpreter-only string ops:**
  - `ai_capability_discovery` and `ai_adaptation_request`, which have no ast op at all
  - `ai_goal_decl`, `ai_knowledge_share` and `ai_tool_chain`. ast spells these `ai_goal_declaration`, `ai_knowledge_sharing` and `ai_toolchain`. **This name mismatch means exo-emitted CASM using them will not lower on ast FastVM.**
  - `push_const_array`
  - `call_host`/`call_interface`, which are placeholders in the interpreter and real only via the FastVM host loop
  - `import_var/export_var`, `gc`, `watchdog/restart`. Both FastVM lowerers accept these, but only the nanovm host services them. CVM1 has none of them.
- **Verdict:** CRUSH-AST AHEAD on breadth. There are no exo-only live semantics worth porting at the opcode level. Two small exceptions: (i) accept nanovm's AI spellings as aliases in ast's FastVM lowerer, if exo CASM is ever fed to it. That touches `ast:fastvm/`, which is **lane-guarded**, so flag it and do not do it unilaterally. (ii) `ai_capability_discovery`/`ai_adaptation_request` are DECLINE, because they only shell out to fleet tools.

### (c) Capability model

- **nanovm.**
  - `Registry{namespaces}` of `Arc<dyn Capability>` (`exo:registry/core.rs:13-92`), where the trait is `ic_id() -> [u8;32]`, `name`, `allows_reentrancy`, `effects() -> Vec<EffectRecord>` and `call(arena,args,hal)` (`exo:traits/core.rs:11-25`).
  - `cap_call` does **no in-VM gating**. It looks the cap up by name and calls it, and a cap error becomes a *string pushed on the stack*, not a trap (`interpreter.rs:908-942`).
  - Effect enforcement `check_effect_permission` (`vm/operations.rs:59`) is **never called**; it is dead.
  - Authority lives in the injected `Hal`. In exosphere that is `ScopedHal` → `CapEngine` (deny-by-default, scoped, audited), in `exo:exo/runtime-core/src/scoped_hal.rs:108-517`, which is **outside nanovm**.
- **ast CVM1.** `HostCaps` of `Box<dyn HostCap>` with `spec()`, `call(args)` and `call_with_deadline` (`ast:host.rs`), plus a portable static registry (`ast:caps.rs`). It has a 3-layer gate: declared (manifest) → allowed (`Quotas.allowed_caps`) → privileged (EXO-205 §4.5, not re-derived here).
- **ast FastVM.** Its local `Capability{name, call(arena,args,hal)}` and an empty marker `Hal` (`ast:fastvm/mod.rs:19-24`) are nanovm's trait minus `ic_id`, `effects` and `reentrancy`.
- **Verdict:** EQUIVALENT in role. ast's CVM1 gate is stricter in the VM than nanovm's. The exo-only part is `ScopedHal/CapEngine`, which is **OUT OF SCOPE → exosphere/exo-light**: per EXO-194 D2, `HostCap` handlers consult CapEngine. ICBF `ic_id`/`effects` have no ast consumer and nanovm never enforces them: DECLINE.

### (d) "PolyglotContext sandboxing"

- `PolyglotContext` is **not in nanovm**. It lives in `exo:crates/core/crush-lang/src/polyglot_imports.rs:12`, and ast has a newer one at `ast:crush-frontend/src/polyglot_imports.rs` (per EXO-205 §7 #2; not re-diffed here, since that belongs to the frontend lane). The CRUSH-55 ticket wording is inaccurate on this point.
- nanovm's actual polyglot sandboxing:
  1. **Allowlist + `exec:<lang>` Wave3 gate**, `enforce_exec_lang_policy` (`exo:vm/mod.rs:228-258`), shared by the interpreter (`interpreter.rs:1476`) and FastVM (`vm/mod.rs:984`). ast equivalent: presence gate `polyglot.<lang>` via `polyglot_gate` (`ast:host.rs:133-151`), with canonical lang names at `scheduler.rs:61-79`. **EQUIVALENT.**
  2. **In-process Lua with restricted stdlib** (`STRING|MATH|TABLE|COROUTINE`, `builtin_executors.rs:44-46`) and **QuickJS with console capture**. ast has no in-process executor. Its live `EXEC_LANG` spawns `python3`/`node`/`bash` subprocesses with wall-clock and process-group kill, optionally bwrap'd via `buckets`. **ast has no Lua at all.** → **PORT candidate (Lua only)**, feature-gated in crush-vm (mlua is a C dep). JS is covered by `node` subprocess plus buckets.
  3. `RuntimeExecutor` plug-in registry, used by the 7 exo `platform/runtimes/*`. It has no ast consumer: DECLINE. exo runtimes migrate to crush-ast walkers per EXO-194 phase 2.
  4. bwrap `NativeRuntime` is dead in the VM (see table). ast's `bucket_exec.rs` is the live equivalent.
- **ast cleanup (separate from any port):** delete or wire the orphaned `ast:crush-vm/src/polyglot/` (2.2k uncompiled lines).

### (e) wasi_bridge, secure_mem, task, debug

- **wasi_bridge.** There is no ast equivalent. `crush-lang-wasm` is a walker/printer, not a value marshaller. It has zero exo consumers, so DECLINE. Also drop the orphan `CbvError` in `ast:crush-errors` if nobody uses it (unverified; it is part C's call).
- **secure_mem.** There is no ast equivalent. It is live code but never switched on in exo. DECLINE, and re-open only with a real confidential-execution requirement. That would be a CVM1 design, not a port of a JSON-instruction page cache.
- **task.** ast green threads cover `spawn/await/yield`. What ast lacks is Erlang-style supervision:
  - `SupervisorPolicy`, restart-on-abort, watchdog timers with actions
  - cancellation cascading to children (`manager.rs:57-322`)
  - a multi-VM round-robin `Scheduler`

  The only exo consumers are vortex's `TaskState` display and a vm-runtime test. DECLINE for now; if the capsule orchestrator wants supervision, it belongs to exo-light/mandala, not the VM.
- **debug.** nanovm has a complete debugger:
  - `StepMode{Into,Over,Out,Instruction}` (`debug/types.rs:529`)
  - watchpoints with `WatchScope` (`:475`)
  - `DebugVisibility{None,ControlOnly,InspectRedacted,Full}`, which redacts values in encrypted mode (`:564`)
  - `StopReason`, event sinks (channel/collecting/tracing) and frame/scheduler snapshots

  ast's `crush-debugger` is a scaffold (4 `todo!()`, breakpoint registry + REPL parser) over `PortableVm::step`/`set_breakpoints` (`portable_vm.rs:233-279`). → **PORT (concepts, not code)**: step-over/out via frame depth, watchpoints, the event-sink trait and visibility levels. nanovm's code is coupled to its `Task`/`VmContext`, so re-implement against `PortableVm`.

### (f) Wave3 gating

`wave3-kernel` is `exo:crates/exo/kernel` (`Kernel::can_grant_blocking(did, cap)`, `kernel.rs:174`: DID registry + held-capability match). nanovm gates **only when a capsule DID and a kernel are attached** through `VM::with_capsule_context` (`vm/mod.rs:484-490`, field `vm/context.rs:49-51`). In production the only attacher is `VmRunner::with_context` from `exo:exo/packman/src/orchestrator.rs:310`, plus the runtime-core tests `exo_87_gate.rs` and `exo_88_gate.rs`. It gates:

1. `ai_query` → `ai.query`, `ai_tool_chain` → `ai.tool_chain`, `ai_agent_delegation` → `ai.agent_delegation` (`interpreter.rs:21-31,1624,1652,1674`)
2. each tool dispatched inside a tool chain → its own cap (`vm/ai.rs:339-351`)
3. `exec_lang` → `exec:<lang>` (`vm/mod.rs:247-255`)

With no DID or kernel attached, everything passes. Known defect EXO-127: `can_grant_blocking` panics inside a tokio runtime (ignored test at `vm/mod.rs:1627`).

ast equivalent: `ai_native.<kind>` HostCap presence gate on CVM1 (`scheduler.rs:1121-1129`) and FastVM (`fastvm/mod.rs:215-243`), plus `polyglot.<lang>`. Same shape: named cap presence, with the authority supplied by the host. **Verdict: OUT OF SCOPE → exo-light.** Its `HostCap` derivation should consult `Kernel`/`CapEngine` (EXO-194 D2). crush-ast must not take a `wave3-kernel` dependency.

### Top PORT candidates (ranked by value)

1. **Debugger semantics into `crush-debugger` + `PortableVm`.** Step over/out, watchpoints, event sink, visibility levels. This fills a scaffold that currently has `todo!()`s and gives vortex-style users a migration target. Roughly 800-1,200 new lines in ast, re-implemented rather than copied from exo `debug/` (3.2k).
2. **AI tool-chain executor as a real `ai_native.toolchain` HostCap.** nanovm `vm/ai.rs:53-390` has sequential, parallel, conditional and retry strategies with per-tool cap checks. ast's AI ops are echo stubs (`fastvm/mod.rs:247-259`). Port the strategy engine into `crush-lang-sdk` (not crush-vm), with tool dispatch through `HostCaps`, and drop the `joker-mcp`/`agent-mem` fallbacks as fleet coupling. Roughly 300-400 lines.
3. **In-process Lua `EXEC_LANG` executor**, behind a new crush-vm feature such as `lua`, with the restricted stdlib. It closes the one language gap (ast has no Lua), and exo's lua runtime and allowlist expect it. Roughly 150-250 lines plus the `mlua` dep. It must plug into `scheduler.rs` exec_lang and the `polyglot.lua` gate. Pair it with deleting the orphaned `ast:crush-vm/src/polyglot/` (-2.2k).
4. **(Optional) CVM1 execution transcript.** A SHA-256 over executed opcodes *and* cap calls, for attestation. nanovm's version is half-wired (no cap calls, bypassed by FastVM), so design it fresh in `scheduler.rs`. Roughly 150 lines, and only if exo-light/NAK attestation wants it.
5. **(Lane-guarded, flag only)** accept nanovm AI op spellings (`ai_goal_decl`, `ai_knowledge_share`, `ai_tool_chain`) as aliases in ast's FastVM lowerer, and decide whether ast's FastVM needs a yield-servicing host loop like `exo:vm/mod.rs:932-1080`. Both sit in `crates/crush-vm/src/fastvm/`, which is off-limits under CRUSH-55's lane guard. About 20 lines for the aliases and about 250 for a host loop.

Everything else is EQUIVALENT, CRUSH-AST AHEAD, DECLINE, DONE (SBL), or OUT OF SCOPE: `lifecycle::ResourceStats` and `pool::SymbolCache` → mandala, `phases` → x-ray, and Wave3 plus ScopedHal/CapEngine → exo-light/exosphere.

---

## Appendix C — vm-runtime, platform/runtimes, crush-common, SDK/ABI


Scope: exosphere `8d52996` (`exo:`) vs the crush-ast working tree (`ast:`). This was a read-only review; nothing was built or run.
"Real" means the code runs the guest program. "Stub" means it returns placeholder data or a TODO. Line counts are Rust `src/` lines.
Verified along the way:
- `ast:crates/crush-vm/src/polyglot/` is **orphaned**. `crush-vm/src/lib.rs` never declares `mod polyglot`, and `crush-vm/Cargo.toml` has no `mlua`/`boa` dependency, so its `LuaExecutor`/`JsExecutor` are not compiled.
- crush-ast's working `@lang` path is `EXEC_LANG`. It runs python, node and bash only (`ast:crates/crush-vm/src/scheduler.rs::resolve_lang_binary`), through a plain subprocess or, with the `sandboxed-polyglot` feature, a buckets/bwrap sandbox (`ast:crates/crush-vm/src/bucket_exec.rs`, CRUSH-20).
- `ast:crates/crush-python` is a name collision. It holds PyO3 bindings for driving crush from Python, not a Python runtime.
- The playbook's W9 row says `core/base/common` holds "time/which/atomicfs/env/paths/proc". **That is wrong.** crush-common has no such modules (see §3). Those helpers belong to `exo:crates/exo/utils` (it has `time.rs`; I did not check the rest).

### 1. `exo:crates/platform/sdk/vm-runtime` (package `vm-runtime`)

**Compiled:** yes. It is a workspace member (`exo:Cargo.toml`).
**Dependents:** only the SDK stack: `capsule-sdk` (re-exports `VM, VmError, VmResult, VmState`), `platform-sdk` and, through them, `crush-sdk`. No runtime crate, daemon or CLI imports it directly.
`lib.rs` is mostly `pub use nanovm::*`. Only the six modules below are its own code.

| Item | lines | real/stub | exo consumers | crush-ast counterpart | Verdict | Note |
|---|---|---|---|---|---|---|
| nanovm re-exports (`lib.rs`) | 93 | shim | capsule-sdk | `ast:crush-vm` | RETIRE | Facade over nanovm. It dies with nanovm. |
| `enforcement.rs` (`Scope`, `CapabilityContext`, `CapabilityEnforcer`, `ScopeGuard`) | 464 | real logic, **unwired** | none. `capsule-sdk/src/enforcement.rs` is an async fork of it (207 diff lines) | `ast:crush-vm/src/portable_vm.rs::dispatch_cap` (declared caps, `Quotas::allowed_caps` exact-name allowlist, `privileged` flag in `ast:crush-vm/src/caps.rs`), `HostCaps` registry and `polyglot_gate` (`ast:crush-vm/src/host.rs`), `HostCapsBuilder` (`ast:crush-lang-sdk/src/host_caps.rs`) | CRUSH-AST AHEAD | exo's enforcer is a standalone checker of scope strings (`fs.*` wildcards, `*`, expiry, check_all/any), and nothing calls it on a VM path. crush-ast enforces inside cap dispatch. Only wildcard and expiry are missing on the ast side. Add them to `allowed_caps` if ever needed (small, optional). |
| `capability.rs` (`CapabilityHandle`, Io/Network/Database/Akg/MessageBus/Task caps) | 391 | **stub** (`CapabilityHandle::call` is `// TODO`, returns `Value::Null`) | crush-sdk (via capsule-sdk fork) | `HostCap` impls in `ast:crush-lang-sdk` (fs, net, db, akg, bus, task) | EQUIVALENT (ast real, exo stub) | Every typed method in exo deserialises `Null`, so all of them fail at runtime. |
| `hal.rs` (`CapsuleHal`) | 23 | stub (`// TODO`) | none | n/a (crush-ast has no HAL; caps are host-provided) | RETIRE | |
| `net.rs` | 372 | stub (`send_request` returns `GatewayUnavailable` unless `gateway-ipc` is on, and that feature also has no transport) | via capsule-sdk (identical copy) | `ast:crush-lang-sdk/src/net.rs` (`net` feature) | RETIRE | Privacy-gateway client for capsules, not a Crush concern |
| `platform.rs` (Platform/Compatibility enums) | 368 | real, trivial | via capsule-sdk (identical copy) | none | OUT OF SCOPE → capsule-contract | Capsule packaging metadata |
| `macros.rs` (`capsule_main!` C-ABI entry points) | 169 | real macro | via capsule-sdk | `ast:crush-vm-capi` (embed direction, not capsule direction) | RETIRE | Emits `crush_capsule_*` symbols that nothing loads |
| `vm_debug/` (breakpoints, watchpoints, step, redacted snapshots, `debug.*` scope gates) | 1,889 | real logic, **unwired** (no caller of `should_pause` outside it) | none | `ast:crates/crush-debugger` (vm_driver, breakpoint, repl, session) | EQUIVALENT / mostly RETIRE | An older fork of `exo:core/vm/nanovm/src/debug/` (3,166 lines, 300+ diff lines per file). Two ideas crush-ast lacks: **encrypted-mode redacted `ValueView`** (type plus content hash) and **cap-gated debug scopes**. Port those only if a debug capability is wanted (S, ~300 lines). |
| `sbl_core.{crush,casm}` | 25+241 | data, not compiled in (no `include_str!`) | none | `ast:crates/crush-lang-sdk/sbl/sbl_core.crush` (CRUSH-122) | DONE | **Byte-identical** to `exo:core/vm/nanovm/src/sbl_core.*` (`diff`: no output). CRUSH-122 already ported the nanovm copy. |
| `vm.rs_stub` | 156 | dead file (not `.rs`) | none | none | RETIRE | Old VM draft importing `exo_core` |
| `tests/` + `src/tests/` + `examples/` | ~2.9k | tests of the nanovm re-exports | none | ast crush-vm tests | RETIRE | Pass/fail not checked (no build). |

### 2. `exo:crates/platform/runtimes/*` (W11)

**Consumers:** all ten are workspace members. `exo:crates/exo/cli/src/loader_bridge.rs` and `exo:crates/exo/vortex/src/shell/mod.rs` register each one as a `nanovm::Capability` (`register_with_nanovm` / `*Capability`). The CLI also has `handlers/js.rs` and `handlers/python.rs`. `runtimes/ai` is used only by `exo:core/base/stdlib/src/lib.rs:375` (the `ai` namespace).
**Shared shape:** each runtime depends on `nanovm`, `crush-common` and `exo-core`, and ships a `nanovm_integration.rs` template (c, go, rust, zig). Each `SandboxPolicy` is advisory. It is enforced only by deno's `--allow-*` flags, Python's module blocklist, and zig's wasmtime fuel.

| Runtime (package) | lines | real/stub | Mechanism | crush-ast counterpart | Verdict | Note |
|---|---|---|---|---|---|---|
| `c` (crush-c) | 1,342 | **mixed** | `Transpile` (default) shells out to `c_walker` → `crush-compiler` and **never executes**; it returns only CAST and CASM sizes. `Native` builds with gcc/clang `-shared`, then **dlopens in-process** (no sandbox). `Zig` mode runs `zig cc`. | `ast:crush-lang-c` (walker) → frontend → VM runs it for real. Native code goes through `ast:crush-vm/src/plugin.rs` (crush-ffi dlopen plugins) and `crush-aot` | EQUIVALENT | ast walkers already do what the transpile mode tried. In-process dlopen of arbitrary C breaks the cap model, so do not port it. |
| `go` (crush-go) | 1,051 | mixed | transpile (same compile-only pattern) / `go run` subprocess with a `timeout` wrapper / FFI | `ast:crush-lang-go` walker. No EXEC_LANG `go` | EQUIVALENT (transpile) / RETIRE (native) | If `@go{}` is ever wanted, add it as a `resolve_lang_binary` / buckets entry (a few lines), not as a crate port. |
| `js` (crush-js) | 1,606 | **real** | default `boa` (embedded boa_engine), `nodejs` / `deno` (deno maps policy to `--allow-*`), `bun` via bun-capsule, `transpile` returns a placeholder (`// TODO`) | EXEC_LANG `js`/`node` (subprocess or buckets `node@20`), `ast:crush-lang-js` walker | EQUIVALENT | Only boa (in-process, no node needed) is missing on the ast side. Not worth the dependency weight: boa caused the icu_normalizer conflict that excluded khukuri-exo. The deno `--allow-*` mapping is a nice idea for a future buckets profile. |
| `js/bun` (bun-capsule) | 247 | real | spawns `bun`. Built on `crush-sdk::Capsule` | none | RETIRE | Capsule wrapper around a subprocess |
| `jvm` (crush-jvm) | 96 | real, thin | writes `Main.java`, runs `java Main.java` | `ast:crush-lang-java` walker. No EXEC_LANG `java` | RETIRE | Unsandboxed subprocess. Re-add as a buckets lang entry if ever needed. |
| `lua` (crush-lua) | 391 | **real** | embedded `mlua` (lua54 vendored). `Sandbox` mode is a **no-op** (full stdlib, `os`/`io` reachable). stdout not captured (`// TODO`) | none compiled (`ast:crush-vm/src/polyglot/builtin_executors.rs::LuaExecutor` is orphaned) | RETIRE (or PORT-lite) | Only in-process embed without a system binary. Port only if a real sandbox (`Lua::new_with` restricted libs) is added. Otherwise delete the orphaned ast executor too. |
| `python` (crush-python) | 3,926 | **real** | `pyo3` in-process (`native.rs`, module blocklist sandbox), `rustpython` backend, `Isolated` worker-process pool (`pool.rs`/`worker.rs`/`bin/python_worker.rs`) with a JSON bridge protocol, pip `packages.rs`, pyo3 DOM tree `dom.rs` (1,077) | EXEC_LANG `python` via subprocess / buckets `python@3.11` plus `pypi:` deps (CRUSH-66) | EQUIVALENT (ast safer) | buckets/bwrap is stronger isolation than a module blocklist. **Idea worth keeping:** the worker's bidirectional **bridge protocol** (guest calls back into host caps mid-run). EXEC_LANG is one-shot stdout. File it as a design note, not a port. `dom.rs` is OUT OF SCOPE → surfer/arniko UI host (same as CRUSH-122's `dom` row). |
| `rust` (crush-rust-runtime) | 1,761 | **stub at execution** | `cargo build` into a temp project plus a sha256 content cache (`cache.rs`), then `run_native` returns `{__binary_path, __input_vars}` placeholders (`executor.rs:185-200`) and never runs the binary | `ast:crush-vm/src/cargo_cap.rs` (`cargo` HostCap), `ast:crush-lang-rust` walker, `crush-aot` | RETIRE | Compiles but never runs. At most borrow the compile-cache keying (~370 lines) if `cargo_cap` needs caching. |
| `swift` (crush-swift) | 82 | real, thin | `swift <tmpfile>` subprocess | none | RETIRE | No walker, no users beyond cli/vortex registration |
| `zig` (crush-zig) | 503 | **effectively broken** | `zig build-exe -target wasm32-wasi`, then wasmtime with fuel. The `Linker` has **no WASI imports**, so instantiating a wasi module should fail (unverified, not run). stdout never captured. `capability.rs` host-function bridge is "Future" | `ast:crush-lang-zig`, `ast:crush-lang-wasm` walkers | RETIRE | The wasmtime-with-fuel sandbox is the only novel part. If WASM guests are ever wanted, design it fresh on `HostCap` + `call_with_deadline`. |
| `ai` (crush-ai-runtime) | 1,479 | **real but fleet-bound** | see §2a | `ast:crush-lang-sdk/src/ai_native.rs` (10 stub caps, CRUSH-32 Done `07f64ee`/`eb28616`), `ast:crush-frontend/src/ai_runtime.rs` | PORT (engine only) | Details below |

#### 2a. `runtimes/ai` vs CRUSH-32

What exo has, file by file:
- `query.rs` (110). `AiQuery` (`ai.query`) spawns `joker-mcp tool joker_smart_query {prompt, system_prompt}` and returns the text. Errors come back as a JSON string, not as an error.
- `toolchain.rs` (412). `execute_tool_chain(registry, arena, tools, strategy, error_handling)`:
  - strategies: `Sequential | Parallel` (a thread per tool on a registry clone), `Conditional{conditions}`, `Retry{max_attempts}`
  - error handling: `FailFast | ContinueOnError | Retry{max_retries} | Fallback`
  - it dispatches each tool by looking up a capability in the registry, then aggregates `{results, aborted, abort_reason}`
  - **No caller** outside the crate. The live path is nanovm's own copy, `exo:core/vm/nanovm/src/vm/ai.rs` (596 lines: `execute_ai_tool_chain` plus goal/progress/knowledge/adaptation), which the in-tree crush-lang compiler targets (`compiler.rs:1251`).
- `delegation.rs` (486). `AiAgentDelegation`:
  - JSON params: task, agents, strategy (`first_available | broadcast | best | round_robin`), expected_format
  - selection reads agent status from `/home/nixp/WORKSPACE/.agent-status`
  - dispatch runs `/home/nixp/WORKSPACE/squadron/bin/foreman-dispatch`
  - it then polls the markdown file `.squad/state/comms/cross_agent_bridge.md` for a `Status: ✅` marker (60 s) and validates the result format
  - **The paths are hardcoded to one machine.**
- `statements.rs` (452). Goal, progress, knowledge (runs `agent-mem`), capability-discovery and adaptation. They write JSON under `~/WORKSPACE/.squad/state`.

What crush-ast has:
- All 10 AI opcodes are routed through `ai_native.<kind>` HostCaps in scheduler, portable_vm and fastvm (`resolve_host_request`), and are stubbed in AOT and JIT.
- The caps are **argc 0 stubs**: VM-stack args are dropped and each returns `{ok, kind, echo}`.
- `ai_runtime.rs` (240) is an unwired data model (goals, adaptations, learning engine, insights). Nothing calls it.
- CRUSH-32 declares real backends a non-goal. CRUSH-122 homes `ai_capabilities` in the "ai-core host capability".

**What to port as the CRUSH-32 follow-up** (call it CRUSH-32b, size M, ~500–700 lines with tests):
1. **Arg plumbing first.** Give the `ai_native.*` specs a real argc/variadic and stop dropping `HostRequest{args}` in `fastvm::resolve_host_request`. Without this no real backend can work.
2. **The ToolChain engine** from `toolchain.rs`: the strategy and error-handling enums plus the sequential, conditional and retry executors. Rebuild it as the body of `ai_native.toolchain`, dispatching each step through `HostCaps::get(name)` (so the grant model applies per tool).
   - Parallel needs `HostCap: Send + Sync`, which it already is.
   - The toolchain cap needs access to the `HostCaps` table (same limitation exo hit with capability discovery). Pass it by construction (`Arc<HostCaps>` snapshot) or handle toolchain in the dispatch loop.
   - Take nanovm `vm/ai.rs::parse_tools` for the CAST argument shape.
3. **Delegation selection logic only** (`select_*`, round-robin counter, `validate_format`), behind a `DelegationBackend` trait.
4. **A `QueryProvider` trait** for `ai_native.query`.

Leave out:
- **Backends: OUT OF SCOPE → ai-core / squadron.** This covers the joker-mcp CLI, foreman-dispatch, bridge-file polling, `agent-mem`, and `.squad/state` writers. They carry fleet box paths and shell out ambiently. Supply them from the host (Antarikshya ai-core) as HostCaps, never from crush-ast.
- `statements.rs` goal/progress/knowledge persistence: DECLINE in crush-ast; it is host state.

### 3. `exo:crates/core/base/common` (crush-common, 3,498 lines incl. hal/event_loop subdirs)

**Deps:** crush-errors, `seahorse-sdk`, tokio, libc/winapi. **Consumers:** about 50 crates (all of `exo/*`, nanovm, stdlib, runtimes, mandala, capabilities, ai).

| Item | lines | real/stub | exo consumers (uses) | crush-ast counterpart | Verdict | Note |
|---|---|---|---|---|---|---|
| `icbf.rs` (ICRecord/TypeRecord/**EffectRecord**/MethodRecord, SemVer, binary codec) | 363 | real | nanovm `Capability::effects()`, stdlib, runtimes/ai (30× `EffectRecord`) | `HostCapSpec` (`ast:crush-vm/src/host.rs`); CRUSH-122 declined `ics` | DECLINE (Crush concern, superseded) | If effect tagging per cap is wanted, add an `effects: &[..]` field to `HostCapSpec` (XS). Don't port ICBF. |
| `host_dispatch.rs` (`HostDispatch` trait, `HostValue`) | 123 | real trait | nanovm `vm/context.rs`, `exo-runtime-core/host_abi.rs` | `HostCap` / `HostCaps` | EQUIVALENT | Same role (VM → host call without dep cycle) |
| `capability.rs` (CapabilityType enum, Handle, ObjectHandle/LifetimeMode, `ScopedCapability` parse `domain.action:scope`, ResourceLimits, CapabilityPhase) | 606 | real | ~35 uses across exo, packman, mandala, cap-engine | `ast:crush-vm/src/caps.rs` (names + privileged), `Quotas` | OUT OF SCOPE → capsule-contract / CapEngine | The grant grammar is an Antarikshya/osmosis policy concern. crush-ast only needs flat cap names. |
| `abi.rs` (capsule Manifest, CapsuleType, ScriptRuntime, ServiceConfig, HealthCheck, RestartPolicy, `Capsule` trait) | 450 | real types | ~75 uses | none (`crush-pkg` manifest is package-level) | OUT OF SCOPE → capsule-contract | Capsule packaging, not language |
| `lifecycle.rs` (canonical capsule state machine, LIFE-01) | 495 | real | daemon, runtime-core, packman, topology, metrics | none | OUT OF SCOPE → capsule-contract / mandala | |
| `hal/mod.rs` (Hal + 14 sub-traits: memory, cpu, fs, net, process, ai, …) + `hal/dummy.rs` (DummyHal) | 681 | traits real; DummyHal a no-op | 54× `Hal`, 43× `DummyHal` (nanovm `Capability::call` takes `Arc<dyn Hal>`) | none (crush-ast has no HAL; host caps replace it) | RETIRE with nanovm | Exo-internal kernel abstraction. Not generic enough for nixpt-common. |
| `hal/seahorse.rs` (`SeahorseHalAi`) | 166 | real client | **none** (only comments in `exo-runtime-core/scoped_hal.rs`) | none | RETIRE (dead) | Pulls `seahorse-sdk` into every crush-common consumer for nothing |
| `event_loop` (+ epoll, tests) | 430 | real (epoll) | 1 use: `exo:crates/exo/hal/src/native/mod.rs` | none | OUT OF SCOPE → exo-hal (move it next to its only user), or W9 nixpt-common if a second user appears | Generic OS utility, not Crush |
| `lib.rs` re-exports of crush-errors | 184 | — | — | `ast:crates/crush-errors` | EQUIVALENT | |

**Net for W9:** nothing in crush-common is a time/which/atomicfs/env/paths/proc helper. The only generic utility is `event_loop`. **Net for crush-ast:** nothing to port. `icbf` and `host_dispatch` are Crush concerns that crush-ast has already replaced with `HostCapSpec`/`HostCap`.

### 4. SDK / ABI and "Wave3"

| Item | lines | real/stub | exo consumers | crush-ast counterpart | Verdict | Note |
|---|---|---|---|---|---|---|
| `exo:crates/platform/abi-c` (crush-abi-c; cbindgen header) | 719 | **mostly stub**: `crush_capsule_init/run` TODO, `crush_capability_request/call` return `NotImplemented`. The fs, io, mem, env and sleep calls are **ambient libc passthroughs with no capability check** | optional feature in vm-runtime, crush-sdk and capsule-sdk only (never on by default) | `ast:crates/crush-vm-capi` (embed VM from C; its doc cites this crate as the pattern), `ast:crates/crush-ffi` (plugin ABI) | **CONFIRM leave/RETIRE** | The capability entry points were never implemented, and the ambient fs/env calls contradict the cap model. |
| `exo:crates/platform/sdk/crush-sdk` | 737 (+tests) | re-exports capsule-sdk plus `gui.rs` and `wave3.rs`, all built on the **stub** `CapabilityHandle::call` (returns `Null`) | joker, lsp, agent-core, janitor, librarian, learn, bun-capsule, tests | none needed | **CONFIRM leave** (capsule-contract replaces) | Not a Crush-language SDK despite the name. It is a Rust capsule-authoring SDK whose typed caps can't work. |
| `crush-sdk/src/wave3.rs` | 402 | stub (typed wrappers over `handle.call`) | none use `WalletCapability`/`IdentityCapability` beyond re-export (`grep`) | none | RETIRE → OpenKO identity/wallet if anything | Wallet (connect/sign/send tx/contracts) and DID/VC identity capability *types* |

**What "Wave3 gating" is** (from `grep -ri wave3`): "Wave3" is exosphere's name for its sovereign-identity and capability kernel layer. The parts:
- `wave3-kernel` is `exo:crates/exo/kernel`: the capability registry and "Sovereign Application Kernel"
- `wave3-macros` is `exo/macros`, and `wave3-sdk` is `platform/sdk/wave3-sdk` (28 lines)
- `exo-daemon`'s `Wave3Verifier` (`exo:crates/exo/daemon/src/wave3_verifier.rs`) checks each capsule's did:key/Ed25519 identity per request (EXO-80) and validates capability delegations
- `cap-engine` stamps grants with `Wave3Origin`

"Wave3 capability gates" (README) means per-opcode checks such as `require_ai_capability()` on `ai.query`, `ai.tool_chain` and `ai.agent_delegation` (EXO-87), resolved against that identity-bound registry. `crush-sdk/wave3.rs` is just the capsule-side typed API for the wallet and identity caps. It is **not a Crush-language concern**. The playbook row already routes kernel, macros and wave3-sdk to "leave" (replaced by `mandala::engine::ControlPlane` + `exo-daemon`), and I agree.

### Top PORT candidates (ranked)

1. **AI ToolChain engine plus arg plumbing → `ast:crush-lang-sdk/src/ai_native.rs` (+ `crush-vm/src/fastvm/mod.rs::resolve_host_request`, cap specs).** Size **M**, ~400–600 lines with tests. Sources: `exo:runtimes/ai/src/toolchain.rs` and `exo:core/vm/nanovm/src/vm/ai.rs::parse_tools`. This is the real CRUSH-32 follow-up. Backends stay host-supplied.
2. **Delegation selection plus `QueryProvider`/`DelegationBackend` traits → `ast:crush-lang-sdk/src/ai_native.rs`.** Size **S**, ~150–200 lines. Take only the selection strategies and format validation from `delegation.rs`. Drop foreman-dispatch and `/home/nixp` paths.
3. *(optional)* **Debugger ideas → `ast:crates/crush-debugger`.** Size **S**, ~300 lines: redacted value views (type + hash) and cap-gated `debug.*` scopes from `exo:vm-runtime/src/vm_debug/snapshots.rs`. Only if encrypted or sandboxed debugging becomes a requirement.
4. *(optional)* **Scope wildcard and expiry in `Quotas::allowed_caps`.** Size **XS**, ~50 lines, from `exo:vm-runtime/src/enforcement.rs::Scope::matches`.
5. *(design note, not code)* **Python worker bridge protocol** (`exo:runtimes/python/src/worker.rs` + `bin/python_worker.rs`): guest→host cap callbacks during an EXEC_LANG run. Worth a CRUSH ticket before anyone rebuilds it.

Everything else in this area is EQUIVALENT, DONE (sbl_core), or RETIRE.

### W11 recommendation

- **python**: EQUIVALENT. ast EXEC_LANG plus buckets (CRUSH-20/66) replaces it with stronger isolation. Retire exo's copy. Keep the bridge protocol as a design note (candidate 5 above). `dom.rs` → surfer/arniko.
- **js**: EQUIVALENT via EXEC_LANG `node` plus buckets. Retire boa, deno and bun.
- **ai**: PORT the engine (candidates 1 and 2 above) into `crush-lang-sdk::ai_native`. Backends go OUT OF SCOPE → Antarikshya ai-core.
- **c, go**: EQUIVALENT. `crush-lang-c`/`-go` walkers plus the real VM supersede the compile-only transpile mode. Retire the native modes (in-process dlopen, `go run`).
- **rust**: RETIRE. It never executes; `cargo_cap` and `crush-aot` cover the need.
- **zig**: RETIRE. Its wasm path lacks WASI imports and is likely broken (unverified).
- **lua**: RETIRE. Its "sandbox" is a no-op. Also delete the orphaned `ast:crush-vm/src/polyglot/` (uncompiled Lua/JS executors) or wire it up deliberately.
- **jvm, swift**: RETIRE. Each is a thin unsandboxed subprocess. Re-add either one as a single `resolve_lang_binary` / buckets entry if ever demanded.
- **js/bun (bun-capsule)**: RETIRE, along with crush-sdk.
- **Hosting rule for Antarikshya:** host exactly the EXEC_LANG set (python, node, bash) through buckets. Every new language is a lang→bucket-spec entry, not a runtime crate.
