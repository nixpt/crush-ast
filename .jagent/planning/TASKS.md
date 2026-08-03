# TASKS — crush-ast

Refreshed 2026-07-25 (docs + CRUSH-66 filing): `main` @ `5fb5bff` includes M2
JIT Phases 2–7 (PR #21). CRUSH-20 ticket status corrected to **Done**. New
Ready item **CRUSH-66** (pypi/npm `@lang[deps]` via BUCKETS-15). Prior
s388 refresh note below still applies for M1 ticket hygiene.

Refreshed s388 (2026-07-16): every open item below was either re-verified against
current `main`, or is a genuinely-still-open ticket. Previously this file had ~60
lines of unstructured findings dumped under "Aspirational" that were neither
aspirational nor current — several described bugs already fixed by unrelated work
(the CRUSHAST-RELEASE-1 arc, this session's merge wave). Don't trust a stale
"critical"/"P0" label without re-running the repro first — see `RULES.md` §1.

See `.jagent/planning/tickets/` for full detail on every `CRUSH-N` ID referenced
here. See `RULES.md` for the worktree/branch/commit discipline every agent
working this backlog must follow.

## Ready

| ID | Task | Notes |
|----|------|-------|
| [CRUSH-66](./tickets/CRUSH-66-lang-deps-pypi-npm.md) | `@lang[pypi:/npm:]` deps via buckets | Design: `docs/design/lang-deps-pypi-npm.md`. Blocked on buckets#4 (BUCKETS-15). |

For the next-arc milestones **M5–M11**, this file tracks only milestone-level
status (one short paragraph per milestone). For full ticket-level detail,
see `.jagent/planning/ROADMAP.md` (the canonical milestone specifications)
and `.jagent/planning/tickets/CRUSH-NN-*.md` (individual ticket files) —
per the ROADMAP's own instructions: don't duplicate ticket content here,
milestone tracking only.

## P0 — Build & Core Health ✅

- [x] **CRUSH-26**: `Build (release)` CI job fails workspace-wide on every
  `main` push for 24+ hours (last 10 consecutive runs, since ≥2026-07-19T22:16)
  — `ort-sys` can't find a native `onnxruntime` static lib on CI's clean
  runners (`ort` is a default feature of `crush-vm` via `native-plugins`).
  Local builds pass silently because a system `onnxruntime` happens to be
  present here — that's why it went unnoticed. Not caused by any recent PR;
  pre-existing. See ticket for fix options (likely: enable `ort`'s
  `download-binaries` feature). (verified done via 008bf91, ort
  download-binaries; checkbox was stale — s412)
- [x] `--all-features` build fixed (rustls dep:)
- [x] `--no-default-features` build (crush-net needs cfg gates)
- [x] Core crates published (casm, crush-cast, crush-errors, crush-vm, crush-frontend, crush-lang-sdk)
- [x] **LTO enabled**: 3-layer (Rust fat LTO + gcc -flto + CFLAGS -flto). Binary size 64-80% reduction (53-142MB → 19-30MB)
- [x] **CRUSH-2** (polyglot capability bypass) — verified fixed s388, `polyglot_gate()` gates `EXEC_LANG` in both scheduler.rs and portable_vm.rs
- [x] **CRUSH-10** (AOT Rust backend can't compile anything) — verified fixed s388, compiles + executes correctly
- [x] **CRUSH-16** (P1): `cargo test --workspace` link failure — fixed by `lto = "thin"` and single crate-type for crush-python.

## M1 — Correctness sweep (black-box bugs found porting real examples)

Every item here was found by actually running programs against the toolchain,
not by source-diving. **Re-verify each repro before fixing** — this session
found 2 of the "P0 critical" tickets in this exact folder were already fixed
by unrelated work; don't assume a ticket's Backlog status means the bug still
reproduces.

- [ ] **CRUSH-1** (L): Wire 10 AI-native opcodes + spawn/await/yield to real VM execution (currently all NOP). Blocks crush-notebook's AI-native cells.
- [x] **CRUSH-7** (M): Array mutation effectively unusable — index-assignment fixed, chained `.push()` fixed (scheduler/portable return array), array slice syntax (`xs[1:]`, `xs[1:3]`) implemented. Nested indexing still open per ticket Resolution.
- [x] **CRUSH-8** (S): Two shipped example files (`fibonacci.crush`, `arrays_and_loops.crush`) — fixed: recursive type inference (Null→Any in BinaryOp + merge_types Any compatibility), for-loop continue target (continue_indices patching), ARR_GET string indexing support
- [x] **CRUSH-9** (L): JS-walked CAST type-inference bugs — root cause was same as CRUSH-8: recursive/forward function calls returned Null placeholder types during inference, causing spurious type errors. Fixed by lenient Null handling in BinaryOp and Any compatibility in merge_types.
- [x] **CRUSH-11** (M): AOT C backend's string-output garbling — **fixed in M1 session**. Root cause: `_add` reset `_strbuf_idx=0` overwriting previously stored strings. Fix: ring-buffer append in `_add`, `_str_dup` in `store`, plus `str_to_upper/lower/trim`. Verified: all 5 backends agree on recursive multi-function string concat (turtle_runner-style).
- [x] **CRUSH-12** (M): Any `struct` declaration silently kills `main` — re-verified; already fixed by unrelated prior work.
- [x] **CRUSH-13** (L): Five independent arithmetic implementations (scheduler/portable_vm/fastvm/aot-rust/aot-c) disagree on div/mod-by-zero (loud error vs. silent 0) and likely other operators. The bugarium flagship differential-testing target; `crush-diff` harness exists but doesn't yet cover the AOT backends.
- [x] **CRUSH-14** (S): `io.print` emits no trailing newline — fixed in scheduler.rs and portable_vm.rs; test expectations updated.
- [x] **CRUSH-15** (S): `crushc --emit casm` text + `crush-run` CASM assembler — **verified working M1 session**. Round-trip tested successfully: basic arithmetic, strings, function calls, recursive functions with conditionals all produce correct output via `crush-run run <file.casm>`. The text format and the assembler accept the same dialect.
- [x] **CRUSH-17** (S): Parser error messages leaked `Token`'s Debug format — fixed s388, added `Token::describe()`/`Display`, 30 call sites updated, verified live + 91 tests green.
- [x] **CRUSH-18** (M): Polyglot block runtime errors (`@python`/`@javascript`/`@bash` guest exceptions) aren't mapped into crush's diagnostic system — **fixed s390** (panini-crush dispatch, foreman-finished after the horse died at max-turns). New `VmError::LangRuntimeError { lang, message, crush_line }` via a shared `lang_runtime_error()` helper in `scheduler.rs`, applied to both `scheduler.rs` and `portable_vm.rs`'s `EXEC_LANG` handlers. `crush_line` threaded from the parser (`parse_lang_block`) through the compiler's spec. Verified against the ticket's own repro end-to-end: `@python { 1/0 }` now renders `"@python block raised a runtime error: (at .crush line 2) ... ZeroDivisionError"` instead of `"unknown capability"`. `crush-ast` `89620e4`.
- [x] **CRUSH-19** (M): `CAP_CALL` has no wall-clock timeout — **fixed s390** (panini-crush dispatch, foreman-finished). Added `HostCap::call_with_deadline()` (cooperative timeout — Option 2 from the ticket; Option 1's `Value: Send` refactor was ruled too invasive for this pass) with a zero-touch default delegating to `call()`. A blocking `HostCap` overrides it and self-enforces `Quotas::max_wall_time_ms`, returning `HostCapError::Timeout` → `VmError::CapTimeout`. Regression test constructs a genuinely-blocking `HostCap` and asserts a prompt timeout, not a hang. `crush-ast` `89620e4`.
- [x] **CRUSH-20** (L, mini-milestone): Wire `buckets` as a sandboxed 4th polyglot execution path — **fixed s390** (panini-crush's 2nd dispatch, foreman-verified after it died silently mid-run to box-wide memory pressure). New `crates/crush-vm/src/bucket_exec.rs`: `lang_to_bucket_spec` (allowlist mirror of `resolve_lang_binary`), `resolve_with_deadline` (reuses CRUSH-19's cooperative-deadline shape for the provisioning step), `build_sandboxed_command` (provisions via `buckets::resolve_multi` + builds a `bwrap` sandbox via `buckets::sandbox::sandboxed_command`). Wired into both `scheduler.rs` and `portable_vm.rs`'s `EXEC_LANG` handlers behind a new `sandboxed-polyglot` feature (off by default). `@lang[deps]` annotation syntax added (parser + `LangBlock`'s new `deps` field). Sandbox-setup failures map through CRUSH-18's `VmError::LangRuntimeError`, extended with a `LangFailurePhase` (`SandboxSetup` vs. guest exception) to distinguish "bwrap couldn't start" from "the guest program raised inside a working sandbox," per the ticket's own note. Layer-ownership decision (crush-vm owns `buckets` directly, not `crush-lang-sdk`) recorded via `dejavue decision` — the dispatch referenced it in a `Cargo.toml` comment but died before actually writing it. **foreman-verify found and fixed one real bug**: the ignored live-sandbox proof test failed with "invalid args JSON" — root-caused to CASM's string-escape set (`\n \t \" \\` only) being narrower than JSON's, corrupting the sentinel-line marshaling escapes on a JSON→CASM→JSON round trip; fixed the test's escaping and simplified its final assertion (the sentinel→typed-value marshaling it originally asserted was never wired into production `EXEC_LANG` stdout handling on EITHER path, sandboxed or not — a separate, real gap beyond this ticket's stated scope, not chased here). Live-verified after the fix: real network fetch, real bash bottle cached, real `bwrap` sandbox spawn, real captured stdout. `cargo test -p crush-vm`: 128/0/1-ignored (default), 128/0 + the now-passing live test (`--features sandboxed-polyglot`). `crush-ast` `main` `c69d76c`.
- [x] **CRUSH-25** (S): AOT rethrow differential test flake (`aot_rethrow_through_three_functions_agrees_fastvm`) — **fixed s394** (CRUSH-AOT-RETHROW-1, sangam). Reproduced 14/50 process runs panicking `scheduler.rs:397: index out of bounds`. Root cause: `scheduler.rs`'s dispatch loop indexed `code[ip]` with no bounds guard (unlike `portable_vm.rs`, which already had one); a thread's `ip` can run past `code.len()` when `THROW` jumps across a function-call boundary without unwinding stale `call_stack` frames (a separate, pre-existing, already-documented VM limitation — NOT fixed here, see ticket Non-goals), and exactly how far off depends on `program.functions`' `HashMap` iteration order (randomized per-process), which is why it only panicked ~28% of the time. Fix: added the same `if ip >= n { return Err(TruncatedInstruction) }` guard `portable_vm.rs` already had, before scheduler.rs's first `code[ip]` read — the panic is now unreachable regardless of layout; the underlying wrong-value-on-some-layouts bug for the interpreter/portable backends remains open (out of scope, this test only asserts FastVM). Post-fix: 200/200 clean on the specific test, 50/50 clean on the full `differential_aot` suite. Gates: `crush-vm --lib` 128/128 ×2 features · `crush-jit --lib` 79/79 · `crush-aot` full package all green.

## M2 — JIT completion

- [x] Phase 1: Skeleton (stack ops, arithmetic, logic, jumps, locals, 21 tests)
- [ ] Phase 2: Locals & Calls (function calls, store/load, CapCall, CallHost)
  - [x] **CRUSH-24**: JIT `CALL`/`RETURN` dispatch cascade panics on Cranelift's
    `!self.is_sealed(block)` SSA invariant, found on `agent/buffy/CRUSHAST-CRUSH-1`
    (s391, foreman). **Superseded, not fixed (s391)** — that branch is retired
    (worktree removed, local+remote deleted). `main`'s independent, already-merged
    JIT calls implementation (continued by PR #21) solves the same problem via a
    different "frame-relative locals" design; non-recursive CALL/RETURN already
    works there. `CRUSHAST-CRUSH-1`'s other commit (AI-opcode AOT stubs) was
    salvaged separately, cherry-picked to `main` `f49ece5`. See ticket for detail.
- [ ] Phase 3: Data & Caps (MakeList, MakeMap, Index, Len, arena)
- [ ] Phase 4: Exceptions (EnterTry, ExitTry, Throw)
- [ ] Phase 5: ExoLight integration
- [ ] Phase 6: Optimization passes
- [ ] Phase 7: AOT compilation
- [ ] (unfiled) crush-jit silently miscompiles ~55 of 86 FastOps per a cranelift fuzz target disagreement (panini, 2026-07-14) — needs its own ticket before work starts; scope unclear from the one-line finding alone.

## M3 — Debugger completion

- [x] Breakpoint registry, REPL, VM integration, VmDriver abstraction, NDJSON wire consumer
- [ ] Variable inspection (`print <var>`)
- [ ] Source → bytecode sourcemap (crush-frontend integration)
- [ ] Step-by-step state inspection

## M4 — Cross-project integration

- [x] **C↔Crush FFI bridge**: plugin auto-build, test_ffi_gateway_cap passing, libcrush_vm.so
- [ ] Tier-3: Migrate surfer's in-tree Crush runtime → crush-ast
- [ ] Reconcile divergence with exosphere's in-tree crush
- [ ] **CRUSH-23**: Crush embedded in exosphere/nakshatra — exosphere half already mapped by `EXO-194` (DECIDED, passive convergence); nakshatra half is new: it has no engine of its own, but its one real Crush artifact (`tools/build.crush`) already runs on exosphere's frozen in-tree path. Captured, not designed — see ticket.

## M5 — AI-native compiler layer

**Proposed**, `.jagent/planning/ROADMAP.md` M5 spec — annotations as CAST node types; `crush-index` v0; AI opcodes VM-execute; agent `codebase.*` host caps wired; `@exhaustive-match-sites` lint; dejavue ↔ crush-index integration. **8 tickets filed** (CRUSH-27 through CRUSH-34) under `.jagent/planning/tickets/CRUSH-27..34-*.md`. See ROADMAP M5 for full spec.

## M6 — Walker parity & multi-language completeness

**Proposed**, `.jagent/planning/ROADMAP.md` M6 spec — close 7 remaining walker-lowering gaps from VISION.md; unify the split `Frontend`/`LanguageAdapter` trait families (6-crate migration including the `crates/cli`'s `py`/`pyw` → `python_walker` non-existent-crate mapping bug); Java + Kotlin walkers (CRUSH-21 family); walker→AOT pipeline for all 12+ walkers. **5 ticket stubs proposed** (CRUSH-35–CRUSH-65, not yet filed). See ROADMAP M6 for full spec.

## M7 — Runtime hardening & ops tooling

**Proposed**, `.jagent/planning/ROADMAP.md` M7 spec — cooperative wall-clock timeout extended to **all** blocking caps (extension of CRUSH-19 — `IO_READ`, `IO_WRITE`, `NET_CONNECT`, `PROCESS_WAIT`, `HOST_REQUEST`, and any future blocking host cap, in addition to `CAP_CALL`); fuel budgets (default 1B instructions per program); deterministic mode (`HashMap`/`HashSet` → `BTreeMap`/`BTreeSet` behind `deterministic` cfg); `crush-pkg` import firewall (`Import firewall placement — spec early` risk flagged, see ROADMAP risks); snapshot/replay (PortableVM + FastVM, `.cvm-snapshot`); V8 fallback feature (`v8-fallback`); Node.js API compat shim (`require('http')` subset); Embedded RustPython VM lane (`crush-lang-python runtime = "rustpython"`); `exo.*` capability module layer (pass-through mediation). **9 ticket stubs proposed** (CRUSH-40–CRUSH-48, not yet filed). See ROADMAP M7 for full spec.

## M8 — Platform & architecture maturation (CRUSH-22 evolved)

**Proposed**, `.jagent/planning/ROADMAP.md` M8 spec — multi-OS + multi-arch CI matrix (`ubuntu-latest` + `macos-latest` + `windows-latest` + `aarch64-ubuntu` + `riscv64-ubuntu` via `cross`); `crush-aot` + `crush-aotc` + `crush-installer` reached consensus on `target_os` cfg coverage (3 OS-cfg sites reconciled, expanding on CRUSH-22's "two AOT backends" framing); Android API host cap shard (`crush-lang-android`); `wasm32-unknown-unknown` first-class (verify `crush-web` lane, not rebuild); Pi-class default install (`crush-installer`). **⚠ Precondition: CRUSH-26 (CI release build) fixed first** — adding new matrix lanes onto a still-red matrix produces no signal; CRUSH-26 must be resolved before CRUSH-49/50. **5 ticket stubs proposed** (CRUSH-49–CRUSH-53, not yet filed). See ROADMAP M8 for full spec.

## M9 — Cross-project convergence & STDLIB restoration

**Proposed**, `.jagent/planning/ROADMAP.md` M9 spec — Surfer's in-tree Crush runtime fully migrated to `crush-ast` (no dual maintenance; two-wave migration); exosphere divergence reconciled (cross-tree `crush` modules merged via the schema-specific design owned by exo's `[main]/buffy` work); CRUSH-23 nakshatra half finalized (`tools/build.crush` artifact on exosphere's frozen in-tree path recorded as canonical); STDLIB clean-restore of **103** capabilities from `exosphere-1.0.zip` with zero mock markers (each gated by an M5 `@covers` test, not hand-verified); STDLIB mock-rewrite of **46** mock-tainted capabilities from spec (one CRUSH ticket per cap, because rewrites touch behavior). **⚠ Precondition: M5+M6+M7 capability surface stable** (for `@covers`-verified restoration gate). **5 ticket stubs proposed** (CRUSH-54–CRUSH-58, not yet filed). See ROADMAP M9 for full spec.

## M10 — Performance ceiling & optimization

**Proposed**, `.jagent/planning/ROADMAP.md` M10 spec — `crush-jit` 55/86 FastOps miscompile audit closure (per the panini 2026-07-14 finding — needs its own ticket before work starts, scope unclear from the one-line finding alone); JIT Phase 6 (Optimization passes: constant folding, dead-code elimination, inlining of small functions); JIT Phase 7 (AOT compilation from JIT — dump compiled native code back as `.so`); conservative→precise GC cutover (shadow stack → real stack maps, eliminates GC pauses for long-lived programs); ML "GC policy brain" PoC (small on-device ML model proposing heuristic GC selection). **5 ticket stubs proposed** (CRUSH-59–CRUSH-63, not yet filed). See ROADMAP M10 for full spec.

## M11 — Universal native & WASM catalyst

**Proposed**, `.jagent/planning/ROADMAP.md` M11 spec — `wasm_walker` migrates into new `crush-lang-wasm` crate with full walker→AOT path; cross-language inlining across **two distinct** walker inputs verified end-to-end (Python → inlined JS function inside a C-codegen `.so`); Universal native compile CLI mode (`crush compile *.crush hello.py lib.rs build.sh main.zig --emit native`); self-hosting walker pipeline in `crush-notebook` (cells in any of 12+ languages execute through walker→AOT path **inside** the notebook kernel, no subprocess); `crush-notebook` integration tests confirming state-sharing between cells of different languages (the "Jupyter-killer" claim verifiable in CI). **⚠ Precondition: M5+M6+M10+M8 stable** (M11's WASM walker→AOT requires M8's `wasm32-unknown-unknown` lane first-class). **Ticket numbers assigned** during M6→M7→M10 in flight (will land in CRUSH-64+ range). See ROADMAP M11 for full spec.

## Publish lane (blocks crates.io release of the walker family)

- [ ] Version drift: only 9/35 crates use `version.workspace = true`; 6 crates
      (walker-core, cli/"walker", go_walker, zig_walker, dart_walker,
      wasm_walker) hardcode a stale `0.1.0` and have drifted from the
      workspace's `0.3.0`. `walker-core` isn't on crates.io at all, blocking
      10 dependent crates (crush-aot + all 8 crush-lang-* + crush-aotc) from
      publishing. Fix: `version.workspace = true` everywhere + publish
      `walker-core`. Note: `crates/cli`'s package name `walker` is squatted
      on crates.io (unrelated project) — needs a rename to `crush-walker`
      before it can publish (name is otherwise free).
- [ ] The `crush-lang-*` vs `*_walker` naming split reflects two incomplete
      generations of the same `Frontend`/`Walker`/`LanguageAdapter` trait
      unification — 6 crates (bash/custom/nepali/python/rust/zsh) implement
      only the old `Frontend` trait and can't register with
      `AdapterRegistry`. `crates/cli/src/main.rs` maps `py`/`pyw` to a
      `python_walker` crate that doesn't exist. Migrating those 6 onto
      `LanguageAdapter` is real, scoped work — not just a rename.

## 💡 Aspirational / research (not scheduled)

> **Section status** (s394, 2026-07-23): Most items below now have formal
> homes under the new **M5–M11** milestones defined in
> `.jagent/planning/ROADMAP.md` and tracked in the M5–M11 sections above
> (V8 fallback / Node.js shim / RustPython lane / exo.* caps / import
> firewall / fuel / deterministic / snapshot → M7; capsule-aware GC + GC
> policy brain → M10; STDLIB restoration map → M9; **CRUSH-21**
> Java/Kotlin family → M6; **CRUSH-22** build platforms → M8). Items
> remaining here are small backlog (not milestone-class): the
> `Program::serialize(Format::Binary)` rmp-serde bug (binary-only,
> 2 `#[ignore]`'d tests in `casm/src/ecasm.rs`).

- [ ] V8 fallback for dynamic JS (feature-gated, snapshot-based, DevTools) — *now: M7 / [CRUSH-45]*
- [ ] Node.js API compatibility shim (require('http') → CAP_CALL) — *now: M7 / [CRUSH-46]*
- [ ] Embedded RustPython VM lane — *now: M7 / [CRUSH-47]*
- [ ] `exo.*` capability modules — *now: M7 / [CRUSH-48]*
- [ ] Import firewall (now: M7 / CRUSH-43), fuel budgets (now: M7 / CRUSH-41), deterministic mode (now: M7 / CRUSH-42), snapshot/replay (now: M7 / CRUSH-44)
- [ ] Unified capsule-aware GC + ML "GC policy brain" — *now: M10 / [CRUSH-62]+[CRUSH-63]*
- [ ] `Program::serialize(Format::Binary)` (rmp-serde) is broken for any Program with an Instruction (`#[serde(flatten)]` incompatibility) — `Format::Json` works fine, this is binary-wire-format only, 2 tests `#[ignore]`d in `casm/src/ecasm.rs` — *still small backlog, post-M11*
- [ ] STDLIB RESTORATION MAP — 103 of 137 archived capabilities (exosphere-1.0.zip) are clean/restorable with zero mock markers; 46 are mock-tainted and must be rewritten, not restored verbatim (they return plausible-looking fake values). Full breakdown in dejavue. — *now: M9 / [CRUSH-56]+[CRUSH-57]*
- [ ] **CRUSH-21**: Java/Kotlin language family — *now: M6 / [CRUSH-37]+[CRUSH-38] for the Java/Kotlin walkers; ticket kept for the JVM/Android-API bridge sub-shard (deferred; M8 / [CRUSH-52] covers Android host-cap surface but the JVM-guest bridge itself is a separate post-M5 ticket)*
- [ ] **CRUSH-22**: Build platforms & architectures — *now: M8 / [CRUSH-49]+[CRUSH-50]+[CRUSH-51]+[CRUSH-52]+[CRUSH-53]*

## Done this session (s388, for context — see FOREMAN_SESSIONS.md s388 for the full merge-wave writeup)

- 8 branches merged: CRUSHAST-CAPTIMEOUT-1 (EXEC_LANG wall-clock timeout), EXECLANG-PLUGGABLE-1, BUCKETSPIKE-1/2 (buckets sandbox proof), PTX-REBASE-1 (crush-ptx + crush-aotc PTX backend scaffold), WEB-1 (crush-web wasm32 target), COLLECTIONS-RECOVER (Tuple/List/Vector/Set types), PYLOWER-1 (Tier 1 Python try/except/match/comprehension lowering), SNAKE-1 (Snake+Turtle Runner examples, filed CRUSH-7..11)
- [x] **issue** — pyo3 version conflict on main — **fixed s390** (`7d8c0d4`): bumped crush-vm's `python` feature to pyo3^0.23 to match crush-python (already ^0.23). `cargo check -p crush-vm -p crush-python` now resolves clean; `cargo check --workspace` (default features) unaffected either way, confirmed green before and after.
- [ ] **issue** — pyo3 0.23.5 doesn't support this box's Python 3.14 at all: building crush-vm's `python` feature for real (`cargo test -p crush-vm --features python`) fails at pyo3's own build-script version gate ("configured Python interpreter version (3.14) is newer than PyO3's maximum supported version (3.13)"). Only surfaced once the version-conflict above was fixed — was masked before because dependency resolution failed first. `python.rs` (CRUSHVM-PYO3 — the chroma-VM↔crush bridge via `run_blob`, the seam Vega's cross-box chroma work depends on) doesn't use pyo3's `abi3` feature, so the usual `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` escape hatch isn't a clean fit without further changes to that module. Not fixed here — touches actively-developed bridge code on the [zorro] side, needs its owner's call (newer pyo3 release, abi3 adoption, or a pinned-down Python 3.13 toolchain for this feature specifically).  _(foreman, 2026-07-19)_
- [ ] **gap** — CRUSH-65 audit: crush-aotc/src/codegen.rs dispatches math ONLY on cap_call names ("math.floor"), but crush-frontend/src/compiler.rs emits math_* OPCODES (math_floor) for those same builtins. So AOT-via-aotc of any crush program using math.floor/sqrt/etc never hits the cap_math_* arms. Mirror image in crush-aot/src/codegen_c.rs, which handles ONLY the math_* opcode form and silently stubs unknown cap_call to mk_null(). Both fall back to NULL silently -- same wrong-answer-no-error class as the Math.floor case-mismatch bug.  _(panini-crush, 2026-07-24)_
- [ ] **gap** — CRUSH-65 audit: JS Math.random() has NO consumer counterpart anywhere in the workspace -- no math.random builtin arm in crush-frontend/compiler.rs, no MATH_RANDOM opcode, no MathRandomCap in crush-lang-sdk/src/stdlib.rs, no math.random arm in crush-aotc. CryptoRandomCap (host_caps.rs:495) yields random BYTES, not a float in [0,1), so it is not a substitute. lower_swc.rs currently lists Math.random in its passthrough arm, so JS Math.random() silently compiles to a broken load-Math + cap_call-random. Needs a real math.random capability (float in [0,1)) before the JS name can be mapped.  _(panini-crush, 2026-07-24)_
- [ ] **opportunity** — CRUSH-65 audit: builtin-table asymmetry between crush-frontend/src/compiler.rs and crush-lang-sdk/src/stdlib.rs. stdlib registers math.sin/math.cos/math.tan/math.min/math.max/math.pi host caps; compiler.rs has fast-path opcode arms only for pow/sqrt/abs/round/floor/ceil. The others still work (they fall through to the cap_call path) so this is NOT a correctness bug -- but the two tables are independently maintained with no shared source of truth, which is exactly the structure that produced the Math.floor miscompile. A single shared builtin-name registry consumed by lower_swc.rs + compiler.rs + aotc + aot would close the whole class.  _(panini-crush, 2026-07-24)_
- [x] **gap** — CRUSH-65: JS Math.sin/Math.cos/Math.tan absent from lower_swc — **fixed CRUSH-69**.
- [x] **issue** — [CRUSH-70](./tickets/CRUSH-70-aot-parallel-compile-collision.md): concurrent AOT compiles of one program raced on the shared cache dir, because rustc names its thin-LTO intermediates after the crate, not the `-o` file. Any cold `/tmp/crush-aot-cache` + parallel test threads = link failure ("cannot open ...rcgu.o"). Surfaced by bozo's first CI run; invisible locally behind a warm cache. **Fixed**: build in a per-invocation work dir, then publish into the cache.
- [ ] **issue** — CRUSH-65: MATH_FLOOR/CEIL/ROUND/SQRT/ABS/POW all push Value::Float (crush-vm/src/scheduler.rs:664, portable_vm.rs:457), and Display renders a whole float as '465.0' (crush-vm/src/vm.rs:274-279). So JS Math.floor(50.7) prints '50.0' where node prints '50' -- a user-visible semantic divergence from JS for every JS program using Math.*. Also affects Python's math.floor via the same opcodes. Fixing means either making MATH_FLOOR/CEIL/ROUND return Value::Int when the result is integral, or making the JS frontend coerce -- both touch VM semantics shared by every language frontend, so out of scope for CRUSH-65. Noted so the JS-parity work does not rediscover it.  _(panini-crush, 2026-07-24)_
- [ ] **issue** — CRUSH-65: crushc silently parses ANY input file as native crush source regardless of extension. crates/crush-lang-sdk/src/bin/crushc.rs:160 calls crush_frontend::parser::Parser::parse(&source) unconditionally -- there is no extension dispatch and crush-lang-sdk does not even depend on crush-lang-js. So 'crushc docs/benchmarks/compute.js' reports 'Compiled ... (170 instructions, 320 bytes)' and exits 0, because the JS subset in that file happens to also be valid crush syntax; the resulting bytecode then dies at runtime with 'unknown capability: Math.floor'. A user compiling a .js file with crushc gets a confident success message and a broken artifact. The real JS entry points are the js_walker subprocess binary (crush-frontend/src/language_walkers.rs:200) and the in-process js_to_cast path. crushc should either dispatch on extension or refuse non-.crush input.  _(panini-crush, 2026-07-24)_
- [ ] **gap** — squeeze declares crush-vm dep with zero usage (Cargo.toml:32) — dead dep, remove  _(panini-crush, 2026-08-02)_
- [ ] **gap** — crush-visuals-debug-bridge declares crush-vm but only uses crush_debugger — dead direct dep, remove  _(panini-crush, 2026-08-02)_
- [ ] **issue** — crush-notebook casm_to_assembly (kernel/src/main.rs:403-478) silently maps unknown casm opcodes to NOP — wrong programs instead of errors; needs a hard-error arm  _(panini-crush, 2026-08-02)_
- [ ] **issue** — exo-light fabric_executor falls back to fake exit_code:0 success when no crush-run binary found — silent failure mode on binary rename  _(panini-crush, 2026-08-02)_
- [ ] **issue** — casm DebugInfo.source_map correctness bug: record_debug_info_for_function appends per-function pc into one flat vector — source_location_for_pc returns wrong function's location for multi-function programs (compiler.rs:312-319, casm/debug_info.rs:166-168)  _(panini-crush, 2026-08-02)_
- [ ] **gap** — casm dead code: CachedProgram/to_cached (lib.rs:246-610, promises 10-100x, never wired, O(F^2) as written) and ecasm.rs (1039 lines, zero external refs) — wire or delete  _(panini-crush, 2026-08-02)_
- [ ] **opportunity** — SemanticAnalyzer multi-pass (4-14 full walks) exists only to work around HashMap function iteration order — replace with Tarjan SCC reverse-topological inference (semantics.rs:98-137)  _(panini-crush, 2026-08-02)_
- [ ] **opportunity** — Lexer design: Vec<char> whole-source copy + String per token + comments materialized then discarded — byte-span tokens + interner (lexer.rs:252)  _(panini-crush, 2026-08-02)_
- [ ] **opportunity** — No compile cache/incremental unit: every entry point recompiles from source; content-hash casm cache + per-function memoization (lib.rs:75-78)  _(panini-crush, 2026-08-02)_
- [ ] **opportunity** — casm codegen builds every instruction out of serde_json::Value (create_instr, 201 call sites, 254 json! literals) — 4-6 heap allocs per emitted instruction, and consumers re-parse JSON via to_opcode() at load. Design fix: emit the existing typed casm::OpCode enum directly into Vec<OpCode>, keep JSON as a serialization view only. CONTRACT CHANGE: casm instruction-stream shape is the .cvm1/crush-notebook/exo-light/mycelium contract — needs foreman sign-off + coordinated client pass (CRUSH-71 finding #1, top-ranked unimplemented)  _(panini-crush, 2026-08-02)_
- [ ] **opportunity** — Every CAST node carries an always-empty HashMap<String,serde_json::Value> meta (48B inline, ~2x node size; only 1 of 54 parser sites ever inserts). Design fix: packed Span{lo,hi} per node + side table for rare real metadata. CONTRACT CHANGE: crush_cast::Program shape is the nimbus + crush-visuals contract (exhaustive matches) — needs coordinated change (CRUSH-71 finding #2)  _(panini-crush, 2026-08-02)_
- [ ] **opportunity** — SemanticAnalyzer clones full recursive Type on every variable reference (resolve_var, semantics.rs) and (Vec<Type>,Type) on every call expression — fix with &Type/Cow or intern to TypeId(u32). Cheaper now that SCC inference cut the pass count (CRUSH-71 finding #4)  _(panini-crush, 2026-08-02)_
- [ ] **opportunity** — Optimizer clones the whole const-propagation map 1-3x per nested block (If/While/For/TryCatch, optimizer.rs) — O(C*N), quadratic when constants accumulate; values are full Expressions instead of a small ConstVal enum; While bodies get an extra full pre-walk (collect_mutated_vars). Fix: scoped-shadowing delta stack, O(delta) per block (CRUSH-71 finding #6)  _(panini-crush, 2026-08-02)_
- [ ] **opportunity** — mutation_check is O(F^2*C^2) (every caller x every other function x linear rescans per annotation match) on the check_source path — fix with pre-built name->indices maps, O(F+sum C) (CRUSH-71 finding #9)  _(panini-crush, 2026-08-02)_
- [ ] **opportunity** — casm::Function has no constant pool (literals inlined per use), no local slots (name-string lookup per variable access at runtime; compiler tracks declared_vars then throws numbering away — crush-lang-sdk/src/compile.rs re-derives slots at a LATER layer), and record_debug_info_for_function produces 'line 1 col 1' for everything with 2 allocs/instruction. Fix: consts Vec<ConstValue> + PushConst(u16), slot-numbered Load/Store(u16), RLE source map. Touches casm shape = same contract caveat as finding #1 (CRUSH-71 finding #10)  _(panini-crush, 2026-08-02)_
