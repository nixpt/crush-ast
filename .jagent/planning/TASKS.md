# TASKS — crush-ast

Refreshed s388 (2026-07-16): every open item below was either re-verified against
current `main`, or is a genuinely-still-open ticket. Previously this file had ~60
lines of unstructured findings dumped under "Aspirational" that were neither
aspirational nor current — several described bugs already fixed by unrelated work
(the CRUSHAST-RELEASE-1 arc, this session's merge wave). Don't trust a stale
"critical"/"P0" label without re-running the repro first — see `RULES.md` §1.

See `.jagent/planning/tickets/` for full detail on every `CRUSH-N` ID referenced
here. See `RULES.md` for the worktree/branch/commit discipline every agent
working this backlog must follow.

## P0 — Build & Core Health ✅

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
- [ ] **CRUSH-18** (M): Polyglot block runtime errors (`@python`/`@javascript`/`@bash` guest exceptions) aren't mapped into crush's diagnostic system — mislabeled `VmError::UnknownCap` (same variant as "capability not granted"), raw foreign-language traceback dumped verbatim, zero crush-side location. Verified live for both Python (`ZeroDivisionError`) and JS (Node stack trace + version banner).
- [ ] **CRUSH-19** (M): `CAP_CALL` has no wall-clock timeout — `dispatch_cap`/`HostCap::call()` runs synchronously with no bound (confirmed by reading the code). CRUSHAST-CAPTIMEOUT-1 explicitly scoped this out of its own fix. Recommended prerequisite for CRUSH-20.
- [ ] **CRUSH-20** (L, mini-milestone): Wire `buckets` as a sandboxed 4th polyglot execution path. **Already spiked and empirically proven** (`CRUSHAST-BUCKETSPIKE-1`/`-2`, merged — `crates/crush-bucketspike` + `SPIKE_RESULTS*.md` are the receipts: bwrap sandboxing genuinely exercised, marshaling survives intact, real cold/warm latency measured). What's left is production wiring: `@lang[deps]` annotation syntax, a layer-ownership decision (crush-vm vs crush-lang-sdk), the numpy/PyPI-deps reframe (buckets provisions bare runtimes only), and actually swapping `EXEC_LANG`'s host `Command::new` for a buckets-backed sandboxed spawn. See `workspace-meta/plans/2026-07-14-crush-polyglot-via-buckets.md` for the full design.

## M2 — JIT completion

- [x] Phase 1: Skeleton (stack ops, arithmetic, logic, jumps, locals, 21 tests)
- [ ] Phase 2: Locals & Calls (function calls, store/load, CapCall, CallHost)
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

- [ ] V8 fallback for dynamic JS (feature-gated, snapshot-based, DevTools)
- [ ] Node.js API compatibility shim (require('http') → CAP_CALL)
- [ ] Embedded RustPython VM lane
- [ ] `exo.*` capability modules
- [ ] Import firewall, fuel budgets, deterministic mode, snapshot/replay
- [ ] Unified capsule-aware GC + ML "GC policy brain"
- [ ] `Program::serialize(Format::Binary)` (rmp-serde) is broken for any Program with an Instruction (`#[serde(flatten)]` incompatibility) — `Format::Json` works fine, this is binary-wire-format only, 2 tests `#[ignore]`d in `casm/src/ecasm.rs`
- [ ] STDLIB RESTORATION MAP — 103 of 137 archived capabilities (exosphere-1.0.zip) are clean/restorable with zero mock markers; 46 are mock-tainted and must be rewritten, not restored verbatim (they return plausible-looking fake values). Full breakdown in dejavue.
- [ ] **CRUSH-21**: Java/Kotlin language family — new `crush-lang-java`/`crush-lang-kotlin` walkers (same tree-sitter-based shape as `crush-lang-go`) plus, separately, a JVM/Android-API capability bridge for crush capsules on mobile. Captured, not designed — see ticket for the open questions.
- [ ] **CRUSH-22**: Build platforms & architectures (Windows/macOS/Android/RISC-V/Pi, Intel/AMD CPU-or-GPU ambiguity) — CI is `ubuntu-latest`-only today, two AOT backends disagree on OS-cfg coverage, zero arch-specific (`aarch64`/`riscv`) code anywhere. Captured, not designed — see ticket.

## Done this session (s388, for context — see FOREMAN_SESSIONS.md s388 for the full merge-wave writeup)

- 8 branches merged: CRUSHAST-CAPTIMEOUT-1 (EXEC_LANG wall-clock timeout), EXECLANG-PLUGGABLE-1, BUCKETSPIKE-1/2 (buckets sandbox proof), PTX-REBASE-1 (crush-ptx + crush-aotc PTX backend scaffold), WEB-1 (crush-web wasm32 target), COLLECTIONS-RECOVER (Tuple/List/Vector/Set types), PYLOWER-1 (Tier 1 Python try/except/match/comprehension lowering), SNAKE-1 (Snake+Turtle Runner examples, filed CRUSH-7..11)
