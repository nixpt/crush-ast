# TASKS — crush-ast

Refreshed 2026-08-01 (cece): **CRUSH-66 Done** — BUCKETS-15 (`pypi:`/`npm:`
resolvers) merged to buckets `master`, and `@lang[pypi:/npm:]` deps now
provision through the buckets sandbox (lexer char-set fix + `validate_deps`
+ live bwrap proof for `pypi:six` / `npm:is-number`, network-isolated). See
`tickets/CRUSH-66-lang-deps-pypi-npm.md` Resolution.

Refreshed 2026-08-23: CRUSH-66 is **Done** (BUCKETS-15 merged and the
`@lang[pypi:/npm:]` sandbox path verified). M2's implementation arc is
substantially landed through Phases 1-5; formal M2 closure still requires the
remaining conformance audit and Phase 6/7 performance/AOT gates. M1 remains
mostly complete with CRUSH-1, residual builtin/backend findings, and nested
array semantics still open. Prior s388 refresh note below still applies for
M1 ticket hygiene.

Refreshed 2026-08-24: `origin/main`/`v0.3.6` now includes CRUSH-80
(delete dead `CachedProgram`/`ecasm.rs`), CRUSH-115 (`io.read`), CRUSH-116
(`math.random`/`math.random_int`/`math.seed`), CRUSH-117
(`conv.chr`/`conv.ord`), and the CRUSH-119/120 FastVM/compiler fixes. CRUSH-118
remains open as the real user-facing `io.read` demo/proof gate.

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
| [CRUSH-66](./tickets/CRUSH-66-lang-deps-pypi-npm.md) | `@lang[pypi:/npm:]` deps via buckets | **Done 2026-08-01** — BUCKETS-15 merged; lexer + `validate_deps` + live bwrap proof. Design: `docs/design/lang-deps-pypi-npm.md`. |
| [CRUSH-72](./tickets/CRUSH-72-jit-silent-null-catchall-bailout.md) | JIT silent TAG_NULL catch-all → Unsupported error + FastVm fallback | **Done 2026-08-05 (s417)** — merged 13fba6a. Catch-all now returns CompileError::Unsupported; JitEngine::run falls back to FastVm transparently. Exhaustive compile-or-refuse test over all FastOp variants. 96/97 tests pass. |
| [CRUSH-73](./tickets/CRUSH-73-conformance-corpus.md) | Conformance corpus — annotated .crush files + black-box runner | **MVP Done 2026-08-05 (s417)** — merged 337544c. 39 files annotated, xtask runner with expect/expect-error/expect-exit/xfail support. Needs release build for CI; debug VM is ~10-20ms/step. |

For the next-arc milestones **M5–M11**, this file tracks only milestone-level
status (one short paragraph per milestone). For full ticket-level detail,
see `.jagent/planning/ROADMAP.md` (the canonical milestone specifications)
and `.jagent/planning/tickets/CRUSH-NN-*.md` (individual ticket files) —
per the ROADMAP's own instructions: don't duplicate ticket content here,
milestone tracking only.

## Filed — awesome-crush toolchain findings (buffy)

Found by black-box language testing while building the `awesome-crush`
model-comparison entries (a Forth + a Brainfuck interpreter). All Backlog,
all with a reproduction in their ticket file:

- **CRUSH-110** — `import` is a no-op: lowered to an unregistered `module.load` cap; no way to share code across files. (P1)
- **CRUSH-111** — type checker registers only `len`/`print` while `compiler.rs` handles ~25 builtin names; non-dotted builtins (`arr_get`, `make_range`, …) are unreachable ("Undefined function").
- **CRUSH-112** — dotted `array.*`/`str.*`/`math.*` builtins compile to unregistered capabilities (`array.push`/`array.pop`, `str.starts_with`, `math.sqrt`, … fail at runtime).
- **CRUSH-113** — `stdlib` feature is off by default and `--stdlib` silently no-ops: no `conv.*`/`chr`/`parse_int`/`collections.*` in the default build. **[x] Done 2026-10-07 (nimbus, lane A1)** — decision C-1: `stdlib` in `default`, crush-run/crush-repl/conformance register it without a flag (`--no-stdlib` opts out), `--stdlib` without the feature is a hard error; `HostCapsBuilder` default unchanged.
- **CRUSH-114** — `len()` errors on strings in the VM but works in the AOT backend (diverges from `str.len`). **[x] Done 2026-09-04** — shared `crush_vm::str_len` byte-length helper across scheduler/PortableVM/FastVM/JIT + both `str.len` caps; differential test green.

## Filed — new capability requests (s439, captain's ask: "add input support and other things")

CRUSH-115, CRUSH-116, and CRUSH-117 are implemented on `origin/main` as of
`v0.3.6`. CRUSH-118 remains the separate user-facing proof gate for stdin.

- [x] **CRUSH-115** — `io.read`: interactive stdin input. Implemented across CVM1, PortableVM, Rust AOT, C AOT, and AOT-C; piped source-pipeline coverage passes.
- [x] **CRUSH-116** — `math.random`/`math.random_int`/`math.seed`: dependency-free SplitMix64 RNG in stdlib, deterministic default and explicit seeding, native source and JavaScript lowering coverage.
- [x] **CRUSH-117** — `conv.chr`/`conv.ord`: always-on portable codepoint conversion with VM, PortableVM, Rust AOT, C AOT, and AOT-C coverage.
- [x] **CRUSH-118** — a real interactive demo proving `io.read` end-to-end, not just registered. **Done 2026-10-07** — `examples/crush/blackjack_interactive.crush` plays natively (piped stdin) and in headless Chromium via crush-web `Session` (pause on `io.read`, resume on `provide`) and `execute_with({ stdin })`; `crush_vm::InputSource` on `PortableVm`. Scheduler parity is the open gap below.

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

## M0 — Foundation & release hygiene

M0 is effectively complete: the parser → CAST → semantics → optimizer →
compiler → CASM pipeline, CVM1/PortableVM, FastVM, core AOT paths, polyglot
execution, sandbox integration, and the initial release/publish plumbing are
landed. Remaining M0-adjacent work is tracked in the Publish lane: workspace
version normalization, walker-core publication, and the `walker` →
`crush-walker` package rename. Platform matrix work belongs to M8, not M0.

## M1 — Correctness sweep (black-box bugs found porting real examples)

M1 is **mostly complete**, not closed. Every item here was found by actually
running programs against the toolchain, not by source-diving. **Re-verify each
repro before fixing** — this session found 2 of the "P0 critical" tickets in
this exact folder were already fixed by unrelated work; don't assume a ticket's
Backlog status means the bug still reproduces.

- [ ] **CRUSH-1** (L): Wire 10 AI-native opcodes + spawn/await/yield to real VM execution (currently all NOP). Blocks crush-notebook's AI-native cells.
- [ ] **CRUSH-7** (M): Array mutation is mostly repaired — index-assignment, chained `.push()`/`.append()`, and the FastVM loop path are fixed. Nested indexing and slice syntax remain open per the ticket Resolution. CRUSH-119/120 closed the FastVM loop/regression portion, not the whole residual ticket.
- [x] **CRUSH-8** (S): Two shipped example files (`fibonacci.crush`, `arrays_and_loops.crush`) — fixed: recursive type inference (Null→Any in BinaryOp + merge_types Any compatibility), for-loop continue target (continue_indices patching), ARR_GET string indexing support
- [x] **CRUSH-9** (L): JS-walked CAST type-inference bugs — root cause was same as CRUSH-8: recursive/forward function calls returned Null placeholder types during inference, causing spurious type errors. Fixed by lenient Null handling in BinaryOp and Any compatibility in merge_types.
- [x] **CRUSH-11** (M): AOT C backend's string-output garbling — **fixed in M1 session**. Root cause: `_add` reset `_strbuf_idx=0` overwriting previously stored strings. Fix: ring-buffer append in `_add`, `_str_dup` in `store`, plus `str_to_upper/lower/trim`. Verified: all 5 backends agree on recursive multi-function string concat (turtle_runner-style).
- [x] **CRUSH-12** (M): Any `struct` declaration silently kills `main` — re-verified; already fixed by unrelated prior work.
- [ ] **CRUSH-13** (L): Arithmetic parity is fixed for the covered interpreter/portable/FastVM paths, but the AOT backends are not yet included in the differential closure gate. Keep open until the all-backend comparison is live.
- [x] **CRUSH-14** (S): `io.print` emits no trailing newline — fixed in scheduler.rs and portable_vm.rs; test expectations updated.
- [x] **CRUSH-15** (S): `crushc --emit casm` text + `crush-run` CASM assembler — **verified working M1 session**. Round-trip tested successfully: basic arithmetic, strings, function calls, recursive functions with conditionals all produce correct output via `crush-run run <file.casm>`. The text format and the assembler accept the same dialect.
- [x] **CRUSH-17** (S): Parser error messages leaked `Token`'s Debug format — fixed s388, added `Token::describe()`/`Display`, 30 call sites updated, verified live + 91 tests green.
- [x] **CRUSH-18** (M): Polyglot block runtime errors (`@python`/`@javascript`/`@bash` guest exceptions) aren't mapped into crush's diagnostic system — **fixed s390** (panini-crush dispatch, foreman-finished after the horse died at max-turns). New `VmError::LangRuntimeError { lang, message, crush_line }` via a shared `lang_runtime_error()` helper in `scheduler.rs`, applied to both `scheduler.rs` and `portable_vm.rs`'s `EXEC_LANG` handlers. `crush_line` threaded from the parser (`parse_lang_block`) through the compiler's spec. Verified against the ticket's own repro end-to-end: `@python { 1/0 }` now renders `"@python block raised a runtime error: (at .crush line 2) ... ZeroDivisionError"` instead of `"unknown capability"`. `crush-ast` `89620e4`.
- [x] **CRUSH-19** (M): `CAP_CALL` has no wall-clock timeout — **fixed s390** (panini-crush dispatch, foreman-finished). Added `HostCap::call_with_deadline()` (cooperative timeout — Option 2 from the ticket; Option 1's `Value: Send` refactor was ruled too invasive for this pass) with a zero-touch default delegating to `call()`. A blocking `HostCap` overrides it and self-enforces `Quotas::max_wall_time_ms`, returning `HostCapError::Timeout` → `VmError::CapTimeout`. Regression test constructs a genuinely-blocking `HostCap` and asserts a prompt timeout, not a hang. `crush-ast` `89620e4`.
- [x] **CRUSH-20** (L, mini-milestone): Wire `buckets` as a sandboxed 4th polyglot execution path — **fixed s390** (panini-crush's 2nd dispatch, foreman-verified after it died silently mid-run to box-wide memory pressure). New `crates/crush-vm/src/bucket_exec.rs`: `lang_to_bucket_spec` (allowlist mirror of `resolve_lang_binary`), `resolve_with_deadline` (reuses CRUSH-19's cooperative-deadline shape for the provisioning step), `build_sandboxed_command` (provisions via `buckets::resolve_multi` + builds a `bwrap` sandbox via `buckets::sandbox::sandboxed_command`). Wired into both `scheduler.rs` and `portable_vm.rs`'s `EXEC_LANG` handlers behind a new `sandboxed-polyglot` feature (off by default). `@lang[deps]` annotation syntax added (parser + `LangBlock`'s new `deps` field). Sandbox-setup failures map through CRUSH-18's `VmError::LangRuntimeError`, extended with a `LangFailurePhase` (`SandboxSetup` vs. guest exception) to distinguish "bwrap couldn't start" from "the guest program raised inside a working sandbox," per the ticket's own note. Layer-ownership decision (crush-vm owns `buckets` directly, not `crush-lang-sdk`) recorded via `dejavue decision` — the dispatch referenced it in a `Cargo.toml` comment but died before actually writing it. **foreman-verify found and fixed one real bug**: the ignored live-sandbox proof test failed with "invalid args JSON" — root-caused to CASM's string-escape set (`\n \t \" \\` only) being narrower than JSON's, corrupting the sentinel-line marshaling escapes on a JSON→CASM→JSON round trip; fixed the test's escaping and simplified its final assertion (the sentinel→typed-value marshaling it originally asserted was never wired into production `EXEC_LANG` stdout handling on EITHER path, sandboxed or not — a separate, real gap beyond this ticket's stated scope, not chased here). Live-verified after the fix: real network fetch, real bash bottle cached, real `bwrap` sandbox spawn, real captured stdout. `cargo test -p crush-vm`: 128/0/1-ignored (default), 128/0 + the now-passing live test (`--features sandboxed-polyglot`). `crush-ast` `main` `c69d76c`.
- [x] **CRUSH-25** (S): AOT rethrow differential test flake (`aot_rethrow_through_three_functions_agrees_fastvm`) — **fixed s394** (CRUSH-AOT-RETHROW-1, sangam). Reproduced 14/50 process runs panicking `scheduler.rs:397: index out of bounds`. Root cause: `scheduler.rs`'s dispatch loop indexed `code[ip]` with no bounds guard (unlike `portable_vm.rs`, which already had one); a thread's `ip` can run past `code.len()` when `THROW` jumps across a function-call boundary without unwinding stale `call_stack` frames (a separate, pre-existing, already-documented VM limitation — NOT fixed here, see ticket Non-goals), and exactly how far off depends on `program.functions`' `HashMap` iteration order (randomized per-process), which is why it only panicked ~28% of the time. Fix: added the same `if ip >= n { return Err(TruncatedInstruction) }` guard `portable_vm.rs` already had, before scheduler.rs's first `code[ip]` read — the panic is now unreachable regardless of layout; the underlying wrong-value-on-some-layouts bug for the interpreter/portable backends remains open (out of scope, this test only asserts FastVM). Post-fix: 200/200 clean on the specific test, 50/50 clean on the full `differential_aot` suite. Gates: `crush-vm --lib` 128/128 ×2 features · `crush-jit --lib` 79/79 · `crush-aot` full package all green.

## M2 — JIT completion

M2 implementation is substantially landed, but the milestone is not formally
closed. Phases 1-5 have implementation and regression coverage in the merged
JIT arc; the remaining closure gates are the full FastOp conformance audit,
optimization validation, AOT-from-JIT output, and differential coverage against
AOT-C.

- [x] Phase 1: Skeleton (stack ops, arithmetic, logic, jumps, locals, 21 tests)
- [x] Phase 2: Locals & Calls (function calls, store/load, CapCall, CallHost)
  - [x] **CRUSH-24**: JIT `CALL`/`RETURN` dispatch cascade panics on Cranelift's
    `!self.is_sealed(block)` SSA invariant, found on `agent/buffy/CRUSHAST-CRUSH-1`
    (s391, foreman). **Superseded, not fixed (s391)** — that branch is retired
    (worktree removed, local+remote deleted). `main`'s independent, already-merged
    JIT calls implementation (continued by PR #21) solves the same problem via a
    different "frame-relative locals" design; non-recursive CALL/RETURN already
    works there. `CRUSHAST-CRUSH-1`'s other commit (AI-opcode AOT stubs) was
    salvaged separately, cherry-picked to `main` `f49ece5`. See ticket for detail.
- [x] Phase 3: Data & Caps (MakeList, MakeMap, Index, Len, arena)
- [x] Phase 4: Exceptions (EnterTry, ExitTry, Throw)
- [x] Phase 5: ExoLight integration
- [ ] Phase 6: Optimization passes — closure work is tracked under M10 / CRUSH-60.
- [ ] Phase 7: AOT compilation — AOT-from-JIT dump remains open under M10 / CRUSH-61.
- [ ] (unfiled) crush-jit silently miscompiles ~55 of 86 FastOps per a cranelift fuzz target disagreement (panini, 2026-07-14) — needs its own ticket before work starts; scope unclear from the one-line finding alone.

## M3 — Debugger completion

- [x] Breakpoint registry, REPL, VM integration, VmDriver abstraction, NDJSON wire consumer
- [ ] Variable inspection (`print <var>`)
- [ ] Source → bytecode sourcemap (crush-frontend integration) — UNBLOCKED by CRUSH-74/79 (s418): real parses now stamp line/col meta, casm `source_map` handles multi-function programs with flat vectors; remaining work is the debugger consuming `Program::with_source_map`
- [ ] Step-by-step state inspection

## M4 — Cross-project integration

- [x] **C↔Crush FFI bridge**: plugin auto-build, test_ffi_gateway_cap passing, libcrush_vm.so
- [ ] Tier-3: Migrate surfer's in-tree Crush runtime → crush-ast
- [ ] Reconcile divergence with exosphere's in-tree crush
- [ ] **CRUSH-23**: Crush embedded in exosphere/nakshatra — exosphere half already mapped by `EXO-194` (DECIDED, passive convergence); nakshatra half is new: it has no engine of its own, but its one real Crush artifact (`tools/build.crush`) already runs on exosphere's frozen in-tree path. Captured, not designed — see ticket.

## M5 — AI-native compiler layer

**Partial / active**, `.jagent/planning/ROADMAP.md` M5 spec. CRUSH-27/28/29/31/32/33
are recorded as done in the current backlog index; CRUSH-30 needs a scope ruling
and CRUSH-34 (spawn/await/yield execution) remains in progress. The VM-side
AI opcode contract is still the main gap, so M5 is not closed. See ROADMAP M5
for the full done condition.

## M6 — Walker parity & multi-language completeness

**Proposed**, `.jagent/planning/ROADMAP.md` M6 spec — close 7 remaining walker-lowering gaps from VISION.md; unify the split `Frontend`/`LanguageAdapter` trait families (6-crate migration including the `crates/cli`'s `py`/`pyw` → `python_walker` non-existent-crate mapping bug); Java + Kotlin walkers (CRUSH-21 family); walker→AOT pipeline for all 12+ walkers. **5 ticket stubs proposed** (CRUSH-35–CRUSH-65, not yet filed). See ROADMAP M6 for full spec.

## M7 — Runtime hardening & ops tooling

**Proposed**, `.jagent/planning/ROADMAP.md` M7 spec — cooperative wall-clock timeout extended to **all** blocking caps (extension of CRUSH-19 — `IO_READ`, `IO_WRITE`, `NET_CONNECT`, `PROCESS_WAIT`, `HOST_REQUEST`, and any future blocking host cap, in addition to `CAP_CALL`); fuel budgets (default 1B instructions per program); deterministic mode (`HashMap`/`HashSet` → `BTreeMap`/`BTreeSet` behind `deterministic` cfg); `crush-pkg` import firewall (`Import firewall placement — spec early` risk flagged, see ROADMAP risks); snapshot/replay (PortableVM + FastVM, `.cvm-snapshot`); V8 fallback feature (`v8-fallback`); Node.js API compat shim (`require('http')` subset); Embedded RustPython VM lane (`crush-lang-python runtime = "rustpython"`); `exo.*` capability module layer (pass-through mediation). **9 ticket stubs proposed** (CRUSH-40–CRUSH-48, not yet filed). See ROADMAP M7 for full spec.

## M8 — Platform & architecture maturation (CRUSH-22 evolved)

**Proposed**, `.jagent/planning/ROADMAP.md` M8 spec — multi-OS + multi-arch CI matrix (`ubuntu-latest` + `macos-latest` + `windows-latest` + `aarch64-ubuntu` + `riscv64-ubuntu` via `cross`); `crush-aot` + `crush-aotc` + `crush-installer` reached consensus on `target_os` cfg coverage (3 OS-cfg sites reconciled, expanding on CRUSH-22's "two AOT backends" framing); Android API host cap shard (`crush-lang-android`); `wasm32-unknown-unknown` first-class (verify `crush-web` lane, not rebuild); Pi-class default install (`crush-installer`). **⚠ Precondition: CRUSH-26 (CI release build) fixed first** — adding new matrix lanes onto a still-red matrix produces no signal; CRUSH-26 must be resolved before CRUSH-49/50. **5 ticket stubs proposed** (CRUSH-49–CRUSH-53, not yet filed). See ROADMAP M8 for full spec.

## Captured 2026-09-24 (captain) — from the nanovm / extraction review

- [ ] **CRUSH-121** — a kitchen-shaped GC for `crush-vm` (scoped arenas + precise tracing + owned/returned audit); **step 1: reproduce + fix the root set** (`collect_garbage` roots only the current stack + locals). [ticket](tickets/CRUSH-121.md)
- [ ] **CRUSH-122** — stdlib convergence (W10): **steps 2–4 landed in #61** (2026-09-25: remaining exosphere stdlib families + nanovm SBL as `system.*`, fs sandbox escape fixed, `array.push/pop` lowering); remaining: step 1 = CRUSH-113 (`stdlib` on by default), step 5 = atlas/playbook correction (outside this repo). [ticket](tickets/CRUSH-122.md)
- [ ] **CRUSH-123** — conformance runner: per-file timeout via child process + fix the frontend hang on `examples/crush/ai_agent_ops.crush` (reproduced: >60 s, no output); full corpus must terminate. [ticket](tickets/CRUSH-123.md)

## Filed 2026-10-04 — GitHub issues #64–#78 (pranix, while writing a CAISON parser)

All reproduced on `main` `a8247af` (polyglot #70/#72/#73 confirmed by reading). #78 (lambda `|x| =>`) is a duplicate of **CRUSH-75**.

- [x] **CRUSH-124** (XS): `"\r"` becomes the letter `r` on CVM1 (assembler unescape) — GH #64. [ticket](tickets/CRUSH-124-string-escape-r-lost-on-cvm1.md)
- [x] **CRUSH-125** (M): `&&`/`||` do not short-circuit — GH #65. [ticket](tickets/CRUSH-125-and-or-no-short-circuit.md)
- [x] **CRUSH-126** (M): `throw` across a call boundary runs post-try code twice (no frame unwinding) — GH #66. [ticket](tickets/CRUSH-126-throw-across-call-runs-code-twice.md)
- [x] **CRUSH-127** (XS): uncaught `throw` reported as `unknown capability`; compile errors labelled `[runtime]` — GH #67. [ticket](tickets/CRUSH-127-uncaught-throw-mislabeled-unknown-capability.md)
- [x] **CRUSH-128** (S): prefix `!`/`-` bind tighter than call/index/field (`!f(x)`, `-f(x)` fail) — GH #68. [ticket](tickets/CRUSH-128-unary-prefix-binds-tighter-than-postfix.md)
- [x] **CRUSH-129** (S): top-level `main()` + `fn main` → infinite recursion — GH #69. [ticket](tickets/CRUSH-129-toplevel-main-call-infinite-recursion.md)
- [x] **CRUSH-130** (S): `crushc → .cvm1` skips polyglot marshaling — GH #70. [ticket](tickets/CRUSH-130-crushc-skips-polyglot-marshaling.md)
- [x] **CRUSH-131** (XS): optimizer folds constants across `@lang` blocks — GH #71. [ticket](tickets/CRUSH-131-optimizer-folds-across-lang-blocks.md)
- [x] **CRUSH-132** (S): `crush-aotc --emit rust` silently drops unsupported ops — GH #72. [ticket](tickets/CRUSH-132-aotc-rust-drops-unsupported-ops.md)
- [x] **CRUSH-133** (S): JIT `ExecLang` is inert — GH #73. [ticket](tickets/CRUSH-133-jit-exec-lang-inert.md)
- [x] **CRUSH-134** (M): field access on `any` rejected (params, nested maps); `any` in conditions — GH #74, #77; truthiness unified on every backend. [ticket](tickets/CRUSH-134-field-access-on-any-rejected.md)
- [x] **CRUSH-135** (S): heterogeneous array literals / array `+` — GH #75; mixed literals are `array<any>`, `+` concatenates on every backend. [ticket](tickets/CRUSH-135-heterogeneous-arrays-and-array-plus.md)
- [x] **CRUSH-136** (S): string `<`/`>` type-check but fail at run time — GH #76. [ticket](tickets/CRUSH-136-string-comparison-runtime-error.md)
- [ ] **CRUSH-137** (S): the scheduler drops `main`'s return value, so the differential harness never compares the interpreter's return (found during CRUSH-126). [ticket](tickets/CRUSH-137-scheduler-drops-main-return-value.md)
- [x] **CRUSH-138** (S–M, P1): FastVM binds call arguments in reverse order — `sub(10, 3)` is `-7` (found during CRUSH-125). [ticket](tickets/CRUSH-138-fastvm-call-arguments-reversed.md)
- [x] **CRUSH-139** (S): JIT takes the wrong branch for `if inside && !outside` (found during CRUSH-125). [ticket](tickets/CRUSH-139-jit-wrong-branch-on-and-not.md)
- [x] **CRUSH-140** (S): FastVM's `ExecLang` request carries no variables (found during CRUSH-133). [ticket](tickets/CRUSH-140-fastvm-exec-lang-no-variables.md)
- [ ] **CRUSH-141** (S): `crushc` and `crush-run x.crush` still compile differently — no `cast_enrich`, optimizer opt-in (found during CRUSH-130). [ticket](tickets/CRUSH-141-crushc-pipeline-differs-from-crush-run.md)
- [x] **CRUSH-142** (S): JIT — `len([1,2,3])` is null; integer overflow returns 0 (the two real JIT divergences left once the harness noise was fixed). [ticket](tickets/CRUSH-142-jit-len-and-overflow.md)
- [x] **CRUSH-143** (XS, P0): optimizer dropped assignments made inside `if` branches — `let n = 0; if c { n = n + 1 }; print(n)` printed 0 (found during CRUSH-136). [ticket](tickets/CRUSH-143-optimizer-drops-if-branch-assignments.md)
- [ ] **CRUSH-144** (S): crush-aotc is outside the differential harness; it missed #76 string ordering (found during CRUSH-134). [ticket](tickets/CRUSH-144-crush-aotc-outside-differential-harness.md)
- [x] **CRUSH-145** (S, P1): SET_FIELD contract diverged — map literals broken on FastVM/JIT/AOT; `m.x = v` leaked a stack slot (found during CRUSH-134). [ticket](tickets/CRUSH-145-set-field-contract-diverged.md)
- [x] **CRUSH-146** (XS, P1): JIT `arr_set` didn't push the array back — `a[i] = v` corrupted the JIT stack (found during CRUSH-135). [ticket](tickets/CRUSH-146-jit-arr-set-stack-contract.md)
- [x] **CRUSH-147** (S): salvage + triage of the dirty shared checkout — salvage `salvage/panini/CRUSH-147-20261005-0848`@`c302e83`; the 196 dirty files are a `cargo fmt --all` run (0 unique work; CRUSH-73/19/11 work is not in it). [ticket](tickets/CRUSH-147-shared-checkout-salvage.md)
- [ ] **CRUSH-148** (S): no fmt gate in CI — 202/319 `.rs` files on `main` aren't rustfmt-clean; pin one rustfmt (CI says 1.85, `rust-toolchain.toml` says stable), one mechanical `cargo fmt --all` commit + `.git-blame-ignore-revs`, then a `cargo fmt --all --check` job. Also: `cargo fmt` fails inside `.jagent/worktrees/*` (sibling `buckets` link lands inside the workspace). [ticket](tickets/CRUSH-148-fmt-ci-gate.md)
- [ ] **CRUSH-149** (S): rename `crush-cson` → `crush-caison` (capability `caison.parse`, `cson.parse` deprecated alias until 0.4), retire the old crate name via `crates/crush-cson-shim`. Mechanical only. [ticket](tickets/CRUSH-149-crush-caison-rename.md)
- [ ] **CRUSH-177** (S): delete crush-frontend's hand-written CAISON parser (`parser/cson.rs`) in favour of `caison`'s reference parser; run caison's `conformance/` vectors in CI. Follow-up to CRUSH-149.
- [ ] **CRUSH-178** (M, needs captain): CAISON → Crush value mapping — `node_to_value` drops confidence/annotations and renders `@synthesize` as a placeholder string. Proposal: `{value, confidence, meta}` map only when present; `@synthesize` as a distinct value the host can fill. Follow-up to CRUSH-149.

## Filed 2026-10-07 — CRUSH-150 migration inventory (derby phase 2)

Ordered by relay lane in [`docs/planning/MIGRATION-INVENTORY.md`](../../docs/planning/MIGRATION-INVENTORY.md) §5; 🔒 = captain decision first (§5.1 C-1…C-8). Lane A starts with existing **CRUSH-113** (decision C-1: stdlib default-on). Supersession proposal for CRUSH-56/57/88–97/108 is CRUSH-169.

- [x] **CRUSH-151** (M (~60 turns), lane A2; **done 2026-10-07, nimbus** — 10 coreutils + VM-local `fs.cd` (C-5), shared `FsSandbox`): fs coreutils host capabilities (`fs.ls/cat/pwd/mkdir/rm/cp/mv/touch/find`). [ticket](tickets/CRUSH-151-fs-coreutils-host-caps.md)
- [x] **CRUSH-152** (XS (~15 turns), lane A3; **done 2026-10-07, nimbus** — `AsyncSleepCap` over the shared `sleep_ms`): `async.sleep` as an alias of `time.sleep`. [ticket](tickets/CRUSH-152-async-sleep-alias.md)
- [x] **CRUSH-153** (S (~30 turns), lane A4; **done 2026-10-07, nimbus** — `env.all/home_dir`, `net.http_put/delete/request`, one deadline-aware request path): `env.all`/`env.home_dir` and `http.put/delete/request`. [ticket](tickets/CRUSH-153-env-http-cap-gaps.md)
- [x] **CRUSH-154** (S (~40 turns), lane A5 🔒; **closed 2026-10-07, nimbus — declined per C-6**, `db.*` covers persistence: [docs/design/storage-caps-declined.md](../../docs/design/storage-caps-declined.md)): `storage.*` handle-based store capabilities — port or decline. [ticket](tickets/CRUSH-154-storage-caps-decision.md)
- [x] **CRUSH-155** (S (~20 turns), lane A6; **done 2026-10-07, nimbus** — defaulted `HostCap::effects()` (not a struct field), builder-wide effects table, `crush-run caps --json`): Effect metadata on `HostCapSpec` (optional). [ticket](tickets/CRUSH-155-hostcapspec-effects.md)
- [x] **CRUSH-156** (M (~40 turns), lane B1): `ai_native.*` caps take real arguments. [ticket](tickets/CRUSH-156-ai-native-arg-plumbing.md)
- [x] **CRUSH-157** (M (~60 turns), lane B2): `ai_native.toolchain` strategy engine. [ticket](tickets/CRUSH-157-ai-toolchain-engine.md)
- [x] **CRUSH-158** (S (~30 turns), lane B3): `QueryProvider` / `DelegationBackend` traits + delegation selection. [ticket](tickets/CRUSH-158-ai-provider-traits.md)
- [x] **CRUSH-176** (S, lane C0): PortableVm skipped a jump landing on its own instruction (#94) — a recursive tail call's `RET` returning to the caller's `RET`, `loop: JMP loop`; awesome-crush games now match the scheduler. [ticket](tickets/CRUSH-176-portable-vm-self-landing-jump.md)
- [x] **CRUSH-159** (M (~70 turns), lane C1): Debugger: step over/out and watchpoints over `PortableVm`. **Done 2026-10-07** — `PortableVm::request_step(Into|Over|Out)` by call depth + `add_watchpoint(slot, Frame(d)|Top)` + `last_stop()`; REPL `next`/`finish`/`watch`/`unwatch`/`print <slot>`; bytecode-level (no slot names / line map from the frontend yet). [ticket](tickets/CRUSH-159-debugger-step-watch.md)
- [x] **CRUSH-160** (M (~50 turns), lane C2): Debugger: event sink, redacted value views, cap-gated debug scopes. **Done 2026-10-07** — `debug.step` / `debug.inspect.redacted` / `debug.inspect` grants via `HostCaps::grant_debug`; `Redactor`/`ValueView`/`FrameSnapshot`; `DebugEvent` + `DebugEventSink` (channel, `CollectingSink`); session refuses ungranted commands; REPL shows program output. [ticket](tickets/CRUSH-160-debugger-events-redaction.md)
- [x] **CRUSH-161** (S (~20 turns), lane D1): Make `crush-pkg` publishable (unblocks squeeze). Dry-run green `19368b4` (panini-d); publish pending (foreman). [ticket](tickets/CRUSH-161-crush-pkg-publishable.md)
- [ ] **CRUSH-162** (M (~50 turns), lane F 🔒): In-process Lua `EXEC_LANG` (decision-gated). [ticket](tickets/CRUSH-162-in-process-lua-exec-lang.md)
- [ ] **CRUSH-163** (M (~70 turns), lane F 🔒): FastVM yield-servicing host loop (decision-gated, lane-guarded). **Deferred** (C-3: decide with CRUSH-77; CRUSH-169). [ticket](tickets/CRUSH-163-fastvm-host-loop.md)
- [ ] **CRUSH-164** (XS (~15 turns), lane F): Wildcard + expiry in `Quotas::allowed_caps` (optional). [ticket](tickets/CRUSH-164-allowed-caps-wildcards.md)
- [ ] **CRUSH-165** (S (~30 turns), lane F): CVM1 execution transcript (hash-chained), feature-gated (optional). [ticket](tickets/CRUSH-165-cvm1-execution-transcript.md)
- [ ] **CRUSH-166** (S (~15 turns), lane F): Design note: guest→host capability callbacks during `EXEC_LANG`. [ticket](tickets/CRUSH-166-exec-lang-callback-design.md)
- [x] **CRUSH-167** (S (~40 turns), lane D2 🔒): Fold squeeze's build-then-run flow into crush-pkg (decision-gated). Bare `crush-pkg` = build → run, `132ddd4` (panini-d). [ticket](tickets/CRUSH-167-fold-squeeze-into-crush-pkg.md)
- [x] **CRUSH-168** (XS (~15 turns), lane E1, panini-e — PR pending): Fix stale in-code docs pointing at exosphere/ecasm. [ticket](tickets/CRUSH-168-stale-cross-repo-docs.md)
- [x] **CRUSH-169** (XS (~10 turns), lane E2, panini-e — PR pending): Close the archive-zip stdlib restoration tickets as superseded. [ticket](tickets/CRUSH-169-close-superseded-stdlib-restore.md)
- [x] **CRUSH-170** (M (~60 turns), lane D3): Capability inference: diff used vs declared caps (`crush-pkg check`). Done (panini, 2026-10-08): `crush_vm::capabilities_used` + `crush-pkg check` E-CAPS findings; check compiles the combined program. [ticket](tickets/CRUSH-170-capability-inference-check.md)
- [x] **CRUSH-171** (S (~25 turns), lane D4): Package manifest category / platform metadata. `[capsule] category` + `platforms`, `4d9e03b` (panini-d). [ticket](tickets/CRUSH-171-manifest-category-metadata.md)
- [x] **CRUSH-172** (XS (~15 turns), lane E3): Example capsule: `squad-bridge-peek` (first pure-Crush capsule). Done (panini, 2026-10-08): capsule + `crush-pkg` grant flags + crush-pkg tests in CI. [ticket](tickets/CRUSH-172-example-squad-bridge-peek.md)
- [x] **CRUSH-173** (S (~40 turns), lane E4, panini-e — PR pending): Design notes recovered from the ancestors. [ticket](tickets/CRUSH-173-design-notes-from-ancestors.md)
- [ ] **CRUSH-174** (S (~20 turns), lane E6): Browser playground on `crush-web` — check, then port only if missing. [ticket](tickets/CRUSH-174-browser-playground-check.md)
- [x] **CRUSH-175** (S (~25 turns), lane E5, panini-e — PR pending): `crush doctor` — polyglot runtime health check. [ticket](tickets/CRUSH-175-crush-doctor.md)
- [x] ~~**CRUSH-162** (M (~50 turns), lane F 🔒): In-process Lua `EXEC_LANG` (decision-gated).~~ **Declined** (C-2: no Lua unless someone asks; CRUSH-169). [ticket](tickets/CRUSH-162-in-process-lua-exec-lang.md)
- [x] ~~**CRUSH-174** (S (~20 turns), lane E6): Browser playground on `crush-web` — check, then port only if missing.~~ **Superseded** by crushlang.org/playground + CRUSH-118 (#93; CRUSH-169). [ticket](tickets/CRUSH-174-browser-playground-check.md)

## Filed 2026-10-09 — test-drive sweep (claude)

Every area exercised end to end on `main` `554f077`: frontend + optimizer, all execution engines (interp, PortableVm, FastVM, JIT, AOT rustc/gcc/clang), every polyglot walker, the capability sandbox, dev tooling, and the embedding surface. Root causes found for GitHub #37 and both halves of #92 (CRUSH-187, CRUSH-189). #38 (`@decision revisit-if` hang) no longer reproduces.

- [ ] **CRUSH-179** (P0, S): `crush-vm run` grants `os.cargo` and `__crush_ffi__` ambiently (process exec + dlopen). [ticket](tickets/CRUSH-179-crush-vm-bin-ambient-cargo-ffi.md)
- [ ] **CRUSH-180** (P0, S): fs sandbox escape: writes follow a dangling symlink out of `--fs-root`. [ticket](tickets/CRUSH-180-fs-sandbox-dangling-symlink-escape.md)
- [ ] **CRUSH-181** (P0, S): Default-on stdlib: unbounded allocations abort the host; negative/invalid args panic. [ticket](tickets/CRUSH-181-stdlib-unbounded-alloc-and-panics.md)
- [ ] **CRUSH-182** (P1, XS): `--task` spawns arbitrary processes without `--process`; children outlive the VM. [ticket](tickets/CRUSH-182-task-start-spawns-without-process.md)
- [x] **CRUSH-183** (P1, S): Host caps that return nothing (`fs.write`, `akg.write`, `message_bus.*`, `task.stop`) fail with stack underflow. [ticket](tickets/CRUSH-183-unit-host-caps-stack-underflow.md)
- [ ] **CRUSH-184** (P1, S): `crush-run`/`crush-walk-run` discard all prior output on a runtime error; walk-run exits 0. [ticket](tickets/CRUSH-184-cli-drops-output-on-runtime-error.md)
- [ ] **CRUSH-185** (P2, M): Stdlib correctness sweep: `path.normalize`, `conv.to_bool`, `time.parse`, negative indices, arity, caps listing. [ticket](tickets/CRUSH-185-stdlib-correctness-sweep.md)
- [ ] **CRUSH-186** (P2, XS): String `+` is limited by the *output* quota; wall-clock limit doesn't bound `process.exec`/loops. [ticket](tickets/CRUSH-186-plus-operator-uses-output-quota.md)
- [x] **CRUSH-187** (P0, XS): Function with an early `return` and a reachable end gets no implicit return (`truncated instruction`). [ticket](tickets/CRUSH-187-missing-implicit-return-after-early-return.md)
- [ ] **CRUSH-188** (P1, XS): Codegen function order is nondeterministic (`Program.functions` is a `HashMap`). [ticket](tickets/CRUSH-188-nondeterministic-function-layout.md)
- [x] **CRUSH-189** (P0, S): Optimizer: `2*e`→`e+e` duplicates side effects; identity rewrites ignore types; fold overflow panics; `catch` sees stale constants. [ticket](tickets/CRUSH-189-optimizer-unsound-rewrites.md)
- [ ] **CRUSH-190** (P1, M): Semantic/type and runtime errors in the CLIs carry no line/column. [ticket](tickets/CRUSH-190-cli-errors-lack-source-location.md)
- [ ] **CRUSH-191** (P1, M): Parser leniency: unterminated annotations swallow code; malformed syntax accepted; duplicates silently replace. [ticket](tickets/CRUSH-191-parser-accepts-malformed-input.md)
- [ ] **CRUSH-192** (P2, S): Parser gaps: match-arm comma after block, negative/float patterns, method calls on non-variable receivers. [ticket](tickets/CRUSH-192-parser-match-and-method-gaps.md)
- [ ] **CRUSH-193** (P1, S): Polyglot marshaling: Python read-modify-write not injected; blocks inside `try` get no marshaling. [ticket](tickets/CRUSH-193-polyglot-marshaling-holes.md)
- [ ] **CRUSH-194** (P2, S): `crushc` CLI papercuts: `--check` misses codegen errors, `--emit types` = `--emit ast`, `--invariant-runtime` unrunnable. [ticket](tickets/CRUSH-194-crushc-cli-papercuts.md)
- [ ] **CRUSH-195** (P1, S): `crush-repl` re-runs the whole session on every line (side effects repeat; one error poisons the session). [ticket](tickets/CRUSH-195-repl-reruns-session.md)
- [ ] **CRUSH-196** (P0, M): Walkers: `__crush_ifexpr__`/`__crush_slice__`/`__crush_contains__`/`__crush_is__` are null stubs; ternary lowered as an eager call. [ticket](tickets/CRUSH-196-walker-null-stub-helpers-and-eager-ternary.md)
- [ ] **CRUSH-197** (P0, S): Rust walker: `println!` drops its arguments, tail expressions aren't returned, `..=` is exclusive. [ticket](tickets/CRUSH-197-rust-walker-println-tail-expr-range.md)
- [ ] **CRUSH-198** (P0, S): Go walker silently drops `:=`, `var`, `for`, assignment, `switch`, `defer`. [ticket](tickets/CRUSH-198-go-walker-drops-statements.md)
- [ ] **CRUSH-199** (P0, S): JS walker: `switch`, bare blocks and `finally` silently dropped; `/` is integer division. [ticket](tickets/CRUSH-199-js-walker-silent-drops-and-division.md)
- [ ] **CRUSH-200** (P0, M): Python walker semantics: `/`, `//`, `%`, `range` step, dicts, `print` separators, silently-ignored constructs. [ticket](tickets/CRUSH-200-python-walker-semantics.md)
- [ ] **CRUSH-201** (P0, S): C walker: `for` with `continue` loops forever; `printf` formats and escapes not applied. [ticket](tickets/CRUSH-201-c-walker-for-continue-printf.md)
- [ ] **CRUSH-202** (P0, M): Bash/Zsh walkers: `$((…))`, `$(…)`, `${#s}` printed literally; `[ a -op b ]` always true. [ticket](tickets/CRUSH-202-shell-walkers-expansions-and-tests.md)
- [ ] **CRUSH-203** (P1, S): Zig/Wasm/Dart walkers produce empty or null programs and exit 0; Java panics; Nepali claim is false. [ticket](tickets/CRUSH-203-stub-walkers-exit-zero.md)
- [ ] **CRUSH-204** (P2, S): Walker CLIs: `crush-walk-run` has no stdlib caps; `walker` needs PATH; `--help` treated as a filename; `export-py` writes the tree. [ticket](tickets/CRUSH-204-walker-cli-papercuts.md)
- [ ] **CRUSH-205** (P1, S): C API: `crush_vm_run_casm` returns 0 on runtime errors; the documented embed example prints nothing. [ticket](tickets/CRUSH-205-capi-run-casm-reports-success-on-error.md)
- [ ] **CRUSH-206** (P2, XS): crush-python: `PyInit_crush` vs lib name `crush_python`; `cast_version()` hard-coded; error handling. [ticket](tickets/CRUSH-206-crush-python-module-name-mismatch.md)
- [ ] **CRUSH-207** (P1, M): tree-sitter-crush is out of date: ERROR nodes in 43/43 examples. [ticket](tickets/CRUSH-207-tree-sitter-grammar-drift.md)
- [ ] **CRUSH-208** (P2, S): crush-lint is a heuristic mock: false positives on valid code, writes to `./.dejavue/timeline.jsonl`. [ticket](tickets/CRUSH-208-crush-lint-mock-false-positives.md)
- [ ] **CRUSH-209** (P2, S): Build-from-clone: undocumented `../buckets` sibling; bucketspike absolute path; crush-cast pulls in the VM. [ticket](tickets/CRUSH-209-build-docs-buckets-sibling-and-layering.md)
- [ ] **CRUSH-210** (P3, S): crush-ptx: no CLI path to emit PTX; each kernel param loaded twice. [ticket](tickets/CRUSH-210-ptx-no-cli-and-dead-param-loads.md)
- [ ] **CRUSH-211** (P1, S): Conformance corpus fails 19/37 annotated files; not in CI. [ticket](tickets/CRUSH-211-conformance-corpus-rot-and-ci-gate.md)
- [ ] **CRUSH-212** (P2, S): Tooling papercuts: crush-diff exits 0 on bad paths; crush-tui/xtask ignore `--help`; installer & versions; doc drift. [ticket](tickets/CRUSH-212-tooling-cli-papercuts.md)
- [ ] **CRUSH-213** (P0, S): `i64::MIN / -1` panics or SIGFPEs on every engine; JIT dies with a signal on `/ 0` and `% 0`. [ticket](tickets/CRUSH-213-int-min-div-and-jit-div-zero-signals.md)
- [x] **CRUSH-214** (P0, XS): Rust AOT backend can't compile any program using `/` or `%` (E0308); keyword-named functions emit invalid Rust. [ticket](tickets/CRUSH-214-rust-aot-div-mod-type-error.md)
- [ ] **CRUSH-215** (P0, S): FastVM/JIT `==`/`!=` wrong for strings, bools and null (JIT tictactoe ends immediately). [ticket](tickets/CRUSH-215-fastvm-jit-equality-non-numeric.md)
- [x] **CRUSH-216** (P0, M): AOT C: 256-byte string ring buffer truncates strings; `io.print` stack contract; float literals and formatting. [ticket](tickets/CRUSH-216-aot-c-strings-print-floats.md)
- [ ] **CRUSH-217** (P1, M): Indexing, map access, `len(map)` and value printing differ on every engine. [ticket](tickets/CRUSH-217-indexing-and-printing-diverge-across-engines.md)
- [ ] **CRUSH-218** (P1, S): JIT allows only 8 call frames; JIT float `%` int fails; FastVM turns failed caps into strings. [ticket](tickets/CRUSH-218-jit-frame-limit-and-fastvm-cap-errors.md)
- [ ] **CRUSH-219** (P1, M): VM runtime errors can't be caught by `try`; AOT rejects `try`/`throw`/structs and has no call-depth limit. [ticket](tickets/CRUSH-219-runtime-errors-uncatchable-aot-no-try.md)
- [ ] **CRUSH-220** (P1, S): `crush-diff` misses almost every cross-engine bug: FastVM abstains, JIT/AOT not included. [ticket](tickets/CRUSH-220-crush-diff-coverage-gaps.md)
- [ ] **CRUSH-221** (P2, XS): Float literals with exponents (`1.5e10`, `1e-7`) don't parse. [ticket](tickets/CRUSH-221-float-exponent-literals.md)
- [ ] **CRUSH-222** (P1, S): The `crush-aotc` crate (second C backend) has no caller and is wrong on 33/52 programs (try/catch skipped, `len`, indexing). [ticket](tickets/CRUSH-222-crush-aotc-crate-unreachable-and-wrong.md)
- [x] **CRUSH-223** (P1, S): AOT C: 512-slot value stack silently drops values; array pool exhausted after ~2000 arrays. [ticket](tickets/CRUSH-223-aot-c-fixed-capacity-silent-failures.md)
- [x] **CRUSH-224** (P1, S): `use @lang` imports never reach later blocks; `LangBlock.imports` dropped; `polyglot_imports.rs` has no callers. [ticket](tickets/CRUSH-224-polyglot-imports-dropped.md)
- [ ] **CRUSH-225** (P2, M): Design: polyglot session state across blocks (one subprocess worker per language per run). [ticket](tickets/CRUSH-225-persistent-polyglot-session.md)
- [x] **CRUSH-226** (P1, XS): `EXEC_LANG` skips the declared-caps and `Quotas::allowed_caps` checks. [ticket](tickets/CRUSH-226-exec-lang-ignores-allowed-caps.md)
- [x] **CRUSH-227** (P1, M): AOT C never frees strings/arrays/maps during a run: O(n²) memory for accumulation, pool exhaustion after 2^20 arrays (collector prototyped). [ticket](tickets/CRUSH-227-aot-c-memory-never-reclaimed.md)
- [ ] **CRUSH-228** (P3, XS): AOT C: dead `mk_null()` fallbacks after allocation; one OOM helper; error helper naming. [ticket](tickets/CRUSH-228-aot-c-dead-null-fallbacks-and-error-helpers.md)
- [ ] **CRUSH-229** (P2, S): Float-to-text exists three times (VM, Rust AOT, C AOT); pin them together. [ticket](tickets/CRUSH-229-float-text-three-copies.md)
- [ ] **CRUSH-230** (P3, XS): VM `MAT_MUL` panics on ragged/mismatched matrices; no Crush syntax emits it. [ticket](tickets/CRUSH-230-vm-mat-mul-panics-on-ragged-input.md)
- [ ] **CRUSH-231** (P1, S): CI: the `toolchain: "1.85"` pin is dead (overridden by `rust-toolchain.toml` stable; `rust-version` is 1.95), and 24 of 42 crates — incl. crush-aot's new parity tests — are compile-only (`--no-run`). [ticket](tickets/CRUSH-231-ci-toolchain-pin-and-untested-crates.md)
- [x] **CRUSH-243** (P1, M): Agent-written programs declare their capabilities; `crush-run` shows and checks them before running. [ticket](tickets/CRUSH-243-agent-programs-declare-capabilities-preflight.md)
- [ ] **CRUSH-233** (P1, M): Pause to approve: hosts intercept each capability call (approve / deny / policy file). [ticket](tickets/CRUSH-233-pause-to-approve-capability-calls.md)
- [ ] **CRUSH-234** (P1, M): AI-native calls fail closed; `semantic_switch` is a NOP; no real provider; flagship example crashes. [ticket](tickets/CRUSH-234-ai-native-fail-closed-and-reachable.md)
- [ ] **CRUSH-235** (P2, M): No memory budget in `Quotas`. [ticket](tickets/CRUSH-235-memory-quota.md)
- [ ] **CRUSH-236** (P1, S): Polyglot blocks run unsandboxed by default; pinned deps then silently ignored; sandbox untested in CI. [ticket](tickets/CRUSH-236-polyglot-sandbox-default.md)
- [ ] **CRUSH-237** (P2, S): Polyglot marshaling misses comprehension inputs; analyzer failure is silent. [ticket](tickets/CRUSH-237-polyglot-marshaling-gaps.md)
- [ ] **CRUSH-238** (P2, M): Walkers stub unsupported constructs as Null; walker/differential docs overclaim. [ticket](tickets/CRUSH-238-walkers-stub-unsupported-constructs.md)
- [ ] **CRUSH-239** (P1, S): `io.read()` returns "" for a blank line and for EOF; scripts reading stdin stop early. [ticket](tickets/CRUSH-239-io-read-eof-vs-blank-line.md)
- [ ] **CRUSH-240** (P2, S): No fs.stat / is_dir / append for scripts. [ticket](tickets/CRUSH-240-fs-file-type-size-append.md)
- [ ] **CRUSH-241** (P2, XS): No stderr output; `[steps=…]` on every run; failing caps labelled `unknown capability`. [ticket](tickets/CRUSH-241-stderr-and-run-summary-noise.md)
- [ ] **CRUSH-242** (P2, XS): `process.exec` returns a JSON string, not a value. [ticket](tickets/CRUSH-242-process-exec-returns-json-string.md)
- [x] **CRUSH-231** (P1, S): CI: the `toolchain: "1.85"` pin is dead (overridden by `rust-toolchain.toml` stable; `rust-version` is 1.95), and 24 of 42 crates — incl. crush-aot's new parity tests — are compile-only (`--no-run`). [ticket](tickets/CRUSH-231-ci-toolchain-pin-and-untested-crates.md)
- [x] **CRUSH-232** (P2, S): `casm_to_vm` total over the CASM `OpCode` set: 38 unsupported → 5 (allowlisted with reasons), `spawn` lowering fixed, 7 NOP stubs now pinned. [ticket](tickets/CRUSH-232-casm-cvm1-lowering-completeness.md)

## M9 — Cross-project convergence & STDLIB restoration

**Proposed**, `.jagent/planning/ROADMAP.md` M9 spec — Surfer's in-tree Crush runtime fully migrated to `crush-ast` (no dual maintenance; two-wave migration); exosphere divergence reconciled (cross-tree `crush` modules merged via the schema-specific design owned by exo's `[main]/buffy` work); CRUSH-23 nakshatra half finalized (`tools/build.crush` artifact on exosphere's frozen in-tree path recorded as canonical); stdlib remainder CRUSH-151..155 from the migration inventory (the archive-zip "103 clean / 46 mock" restore, CRUSH-56/57/88–97/108, is superseded by CRUSH-122 — CRUSH-169). **⚠ Precondition: M5+M6+M7 capability surface stable** (for `@covers`-verified restoration gate). **5 ticket stubs proposed** (CRUSH-54–CRUSH-58, not yet filed). See ROADMAP M9 for full spec.

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
- [x] ~~STDLIB RESTORATION MAP (exosphere-1.0.zip restore)~~ — **superseded** (CRUSH-169): CRUSH-122 restored the clean families from exosphere's live tree; the mock-tainted ones are dead/out per `docs/planning/MIGRATION-INVENTORY.md` §2.2; the remainder is CRUSH-151..155.
- [ ] **CRUSH-21**: Java/Kotlin language family — *now: M6 / [CRUSH-37]+[CRUSH-38] for the Java/Kotlin walkers; ticket kept for the JVM/Android-API bridge sub-shard (deferred; M8 / [CRUSH-52] covers Android host-cap surface but the JVM-guest bridge itself is a separate post-M5 ticket)*
- [ ] **CRUSH-22**: Build platforms & architectures — *now: M8 / [CRUSH-49]+[CRUSH-50]+[CRUSH-51]+[CRUSH-52]+[CRUSH-53]*

## Done this session (s388, for context — see FOREMAN_SESSIONS.md s388 for the full merge-wave writeup)

- 8 branches merged: CRUSHAST-CAPTIMEOUT-1 (EXEC_LANG wall-clock timeout), EXECLANG-PLUGGABLE-1, BUCKETSPIKE-1/2 (buckets sandbox proof), PTX-REBASE-1 (crush-ptx + crush-aotc PTX backend scaffold), WEB-1 (crush-web wasm32 target), COLLECTIONS-RECOVER (Tuple/List/Vector/Set types), PYLOWER-1 (Tier 1 Python try/except/match/comprehension lowering), SNAKE-1 (Snake+Turtle Runner examples, filed CRUSH-7..11)
- [x] **issue** — pyo3 version conflict on main — **fixed s390** (`7d8c0d4`): bumped crush-vm's `python` feature to pyo3^0.23 to match crush-python (already ^0.23). `cargo check -p crush-vm -p crush-python` now resolves clean; `cargo check --workspace` (default features) unaffected either way, confirmed green before and after.
- [ ] **issue** — pyo3 0.23.5 doesn't support this box's Python 3.14 at all: building crush-vm's `python` feature for real (`cargo test -p crush-vm --features python`) fails at pyo3's own build-script version gate ("configured Python interpreter version (3.14) is newer than PyO3's maximum supported version (3.13)"). Only surfaced once the version-conflict above was fixed — was masked before because dependency resolution failed first. `python.rs` (CRUSHVM-PYO3 — the chroma-VM↔crush bridge via `run_blob`, the seam Vega's cross-box chroma work depends on) doesn't use pyo3's `abi3` feature, so the usual `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` escape hatch isn't a clean fit without further changes to that module. Not fixed here — touches actively-developed bridge code on the [zorro] side, needs its owner's call (newer pyo3 release, abi3 adoption, or a pinned-down Python 3.13 toolchain for this feature specifically).  _(foreman, 2026-07-19)_
- [ ] **gap** — CRUSH-65 audit: crush-aotc/src/codegen.rs dispatches math ONLY on cap_call names ("math.floor"), but crush-frontend/src/compiler.rs emits math_* OPCODES (math_floor) for those same builtins. So AOT-via-aotc of any crush program using math.floor/sqrt/etc never hits the cap_math_* arms. Mirror image in crush-aot/src/codegen_c.rs, which handles ONLY the math_* opcode form and silently stubs unknown cap_call to mk_null(). Both fall back to NULL silently -- same wrong-answer-no-error class as the Math.floor case-mismatch bug.  _(panini-crush, 2026-07-24)_
- [x] **gap** — CRUSH-65 audit: JS Math.random() had no consumer counterpart; CRUSH-116 now provides `math.random` in stdlib and lowers JS `Math.random()` to that canonical capability. The remaining default-on question belongs to CRUSH-113.  _(resolved 2026-08-23)_
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
- [x] **gap** — casm dead code: CachedProgram/to_cached and ecasm.rs — **fixed CRUSH-80**. Deleted `crates/casm/src/ecasm.rs` and the orphaned `CachedProgram`/`to_cached` remnants from `crates/casm/src/lib.rs`; CRUSH-83 owns any future real compile-cache design.
- [ ] **opportunity** — SemanticAnalyzer multi-pass (4-14 full walks) exists only to work around HashMap function iteration order — replace with Tarjan SCC reverse-topological inference (semantics.rs:98-137)  _(panini-crush, 2026-08-02)_
- [ ] **opportunity** — Lexer design: Vec<char> whole-source copy + String per token + comments materialized then discarded — byte-span tokens + interner (lexer.rs:252)  _(panini-crush, 2026-08-02)_
- [ ] **opportunity** — No compile cache/incremental unit: every entry point recompiles from source; content-hash casm cache + per-function memoization (lib.rs:75-78)  _(panini-crush, 2026-08-02)_
- [ ] **opportunity** — casm codegen builds every instruction out of serde_json::Value (create_instr, 201 call sites, 254 json! literals) — 4-6 heap allocs per emitted instruction, and consumers re-parse JSON via to_opcode() at load. Design fix: emit the existing typed casm::OpCode enum directly into Vec<OpCode>, keep JSON as a serialization view only. CONTRACT CHANGE: casm instruction-stream shape is the .cvm1/crush-notebook/exo-light/mycelium contract — needs foreman sign-off + coordinated client pass (CRUSH-71 finding #1, top-ranked unimplemented)  _(panini-crush, 2026-08-02)_
- [ ] **opportunity** — Every CAST node carries an always-empty HashMap<String,serde_json::Value> meta (48B inline, ~2x node size; only 1 of 54 parser sites ever inserts). Design fix: packed Span{lo,hi} per node + side table for rare real metadata. CONTRACT CHANGE: crush_cast::Program shape is the nimbus + crush-visuals contract (exhaustive matches) — needs coordinated change (CRUSH-71 finding #2)  _(panini-crush, 2026-08-02)_
- [ ] **opportunity** — SemanticAnalyzer clones full recursive Type on every variable reference (resolve_var, semantics.rs) and (Vec<Type>,Type) on every call expression — fix with &Type/Cow or intern to TypeId(u32). Cheaper now that SCC inference cut the pass count (CRUSH-71 finding #4)  _(panini-crush, 2026-08-02)_
- [ ] **opportunity** — Optimizer clones the whole const-propagation map 1-3x per nested block (If/While/For/TryCatch, optimizer.rs) — O(C*N), quadratic when constants accumulate; values are full Expressions instead of a small ConstVal enum; While bodies get an extra full pre-walk (collect_mutated_vars). Fix: scoped-shadowing delta stack, O(delta) per block (CRUSH-71 finding #6)  _(panini-crush, 2026-08-02)_
- [ ] **opportunity** — mutation_check is O(F^2*C^2) (every caller x every other function x linear rescans per annotation match) on the check_source path — fix with pre-built name->indices maps, O(F+sum C) (CRUSH-71 finding #9)  _(panini-crush, 2026-08-02)_
- [ ] **opportunity** — casm::Function has no constant pool (literals inlined per use), no local slots (name-string lookup per variable access at runtime; compiler tracks declared_vars then throws numbering away — crush-lang-sdk/src/compile.rs re-derives slots at a LATER layer), and record_debug_info_for_function produces 'line 1 col 1' for everything with 2 allocs/instruction. Fix: consts Vec<ConstValue> + PushConst(u16), slot-numbered Load/Store(u16), RLE source map. Touches casm shape = same contract caveat as finding #1 (CRUSH-71 finding #10)  _(panini-crush, 2026-08-02)_
- [ ] **issue** — BUG: function ending in if{return;}else{<call>;} with nothing after emits bytecode missing a terminator -> VM crashes 'truncated instruction at N' (N=total instr count). Repro: fn count(n,max){io.print(n); if n>=max{return;}else{count(n+1,max);}} fn main(){count(0,5);} -- crashes. Fix: add trailing 'return;' after the if/else (even though unreachable) makes it work. Found reviewing Ornith-1.5-35B's pong.crush (pong_tick's body ends in exactly this shape, no code bug on its side). Needs a real codegen fix: emit an implicit return/halt when a function body's last statement is an if/else with no fallthrough statement.  _(unknown, 2026-08-20)_
- [ ] **issue** — BUG: crushc hangs (infinite loop, not a crash) parsing @decision's 'revisit-if' field specifically -- isolated via bisection: @module alone compiles fine, @invariant alone compiles fine, @decision with chose/because/over[] all compile fine, but @decision { revisit-if: [...] } alone hangs crushc indefinitely (tested with timeout 10, never returns). Likely the hyphenated field name (revisit-if) confuses the lexer/parser -- 'if' after the hyphen is a reserved keyword, and the parser's error-recovery path for 'identifier, unexpected -, keyword' probably loops instead of erroring. Repro: @decision "test" { chose: "a" revisit-if: ["never"] } fn main() { io.print("hi"); return; } -- crushc hangs with zero output, never reaches instruction-count print. NOT reproduced for @module or @invariant (both compile instantly). This affects docs/design/CRUSH-AI-ANNOTATIONS.md's own documented @decision schema (revisit-if is one of its 4 fields) and examples/crush/ai_agent_ops.crush's own // expect-error: TIMEOUT fixture likely traces to this exact bug, not (only) the semantic_switch/ai_synthesize runtime calls the file's comments imply.  _(unknown, 2026-08-20)_
- [x] **CRUSH-119** — FastVM array mutation and `for` loops — **fixed 2026-08-22**. Native `push`/`append`/`pop`/`arr_get`/`arr_set`/`arr_len` capability forms now lower to FastOps; array mutation returns match CVM1. Regression coverage executes compiled range and array loops, including `continue`.  _(buffy)_
- [x] **CRUSH-120** — Optimizer constant propagation no longer rewrites an assignment target's self-reference before invalidating it. `total = total + item` now preserves loop-carried state in optimized FastVM execution; regression coverage returns `6` for `[1, 2, 3]`.  _(buffy, 2026-08-23)_
- [ ] **gap** — crush-ast has no cargo fmt gate; 197 .rs files unformatted on main; rustfmt version drift changes 5 files. Pin toolchain + add fmt --check CI, one mechanical commit  _(panini, 2026-10-05)_
- [ ] **issue** — CRUSH-73/19(72)/11 'done' work not in shared checkout salvage (it is fmt-only); locate in other worktrees/branches  _(panini, 2026-10-05)_
- [ ] **CRUSH-104** — Make crush-lang-sdk publishable again: ship the `polyglot.<lang>` gate (crates.io SDK 0.2.0 runs `@lang` ungated via crush-vm 0.2.0). 13-crate publish set packages + verifies; gate tests + `Test (sdk)` CI; publish/yank commands in the ticket. Awaiting merge → publish → arniko-crush bump → yank.  _(panini-104, 2026-10-07)_
- [ ] **issue** — arniko-crush 0.3.0 (crates.io, nixpt-owned, published 2026-09-27) depends on crush-lang-sdk ^0.2.0 (default-features=false) — i.e. it pulls the UNGATED crush-vm 0.2.0. Yanking crush-lang-sdk 0.2.0 breaks fresh resolves of arniko-crush 0.3.0. Release arniko-crush 0.3.1 on crush-lang-sdk ^0.3 BEFORE the CRUSH-104 yank.  _(panini-104, 2026-10-07)_
- [ ] **issue** — crates-publish-sync.service runs --repo /workspace/projects/crush-ast (the SHARED checkout), so it publishes whatever HEAD happens to be checked out there, possibly a stale or dirty tree; its timer is disabled and its log is empty. Point it at a clean origin/main clone/worktree before re-enabling it for the CRUSH-104 publish.  _(panini-104, 2026-10-07)_
- [x] **issue** — Internal dep version reqs say 0.3.0 (workspace.dependencies + inline) while consumers need 0.3.8 APIs: e.g. crush-lang-js fails to compile against crates.io crush-cast 0.3.0 (Statement::LangBlock has no deps field; reproduced with a subset cargo package). A downstream lockfile pinned at crush-cast 0.3.0 is not upgraded by cargo and breaks. bump-version.sh never raises dep reqs. Fixed in CRUSH-104: internal path-dep reqs raised to 0.3.8.  _(panini-104, 2026-10-07)_
- [ ] **gap** — Scheduler (crush_vm::run / run_with_caps, the CVM1 path crush-run and crush-web execute() use) still reads io.read from process stdin only; CRUSH-118 added InputSource (supplied/interactive) to PortableVm alone. Thread InputSource into scheduler.rs so embedders on the scheduler path can feed io.read too (supplied mode is trivial; interactive needs a scheduler-level pause). Parity: crush-diff should keep both VMs on one io.read source contract.  _(nimbus-118, 2026-10-07)_
- [ ] **issue** — exosphere capsule-ui crates/platform/sdk/capsule-ui/src/crush/crush-markup.tsx:175 passes raw html prop to dangerouslySetInnerHTML with no sanitizer despite header claiming 'safely' — XSS risk; exosphere-owned (found by CRUSH-150 scout)  _(nimbus-scout, 2026-10-07)_
- [ ] **issue** — EXO-194 hazard 1 has happened: exosphere Cargo.lock holds both in-tree casm 0.1.0 and crates.io casm 0.3.0 (via exo-light -> crush-vm 0.3.6), plus two crush-errors; exo-light also lags crush-ast 0.3.9 (found by CRUSH-150)  _(nimbus-scout, 2026-10-07)_
- [ ] **gap** — crush-debugger README.md:11,43 and lib.rs:27 still describe todo!() hook points that no longer exist (session.rs:346 replaced them) — stale docs  _(nimbus-scout, 2026-10-07)_
- [ ] **gap** — xtask conformance corpus: 23 of 48 annotated examples/crush files fail on main — expect-error headers say '[runtime] …' but the runner reports 'runtime error: …'/'compile error: …', plus truncated '...' headers. The runner is not in CI, so the drift is invisible; fix headers (or normalize prefixes) and gate it  _(nimbus-a, 2026-10-07)_
- [ ] **issue** — crush-lang-sdk's crush-diff bin does not compile with --no-default-features (uses crush_lang_sdk::differential, which is native-plugins-only) — needs required-features = ["native-plugins"] on the [[bin]]. CI's workspace-level feature-gates job hides it via feature unification  _(nimbus-a, 2026-10-07)_
- [x] **gap** — (fixed by CRUSH-159) crush-debugger README.md:11,43 and lib.rs:27 still describe todo!() hook points that no longer exist (session.rs:346 replaced them) — stale docs  _(nimbus-scout, 2026-10-07)_
- [x] **gap** — (fixed by CRUSH-160) crush-debugger never shows the debugged program's output: io.print/PRINT go to PortableVm's buffer and the REPL never drains it (calls.crush prints 11, the session shows only 'done'). Fix: a VmDriver::take_output default + print it after each command. Found during CRUSH-159 live run.  _(nimbus-c, 2026-10-07)_
- [ ] **issue** — crush-buckets public API drifted without a version bump: the buckets checkout's SandboxProfile has extra_rw_binds + net_ns, crates.io crush-buckets 0.1.0 does not, both say 0.1.0. Any crush-ast crate that names those fields builds against the path dep and fails publish verification against crates.io (hit on crush-pkg, CRUSH-161). buckets should publish 0.2.0; crush-ast should use ..Default::default() for SandboxProfile until then.  _(panini-d, 2026-10-07)_
- [ ] **gap** — Crush programs have no way to receive command-line args: crush-pkg's CrushRunner ignores its args (crush-pkg run -- a b and the new bare build-then-run both drop them for Crush capsules), and crush-vm/crush-lang-sdk expose no argv capability (no env.args / sys.args). Script/Native capsules do get args. Needs a design call: a grant-gated argv cap or a main(args) convention.  _(panini-d, 2026-10-07)_
- [x] **issue** — *(fixed by CRUSH-170: check now compiles the program `build` does)* crush-pkg check fails on any package whose entry calls a function from a [[dependencies]] path dep: PackageBuilder::check compiles each source file standalone (compile_crush_to_casm), so 'Undefined function: greet' — while crush-pkg build (which concatenates entry + deps) succeeds. check should compile the same combined program build does. Repro: crush-pkg new app/util, util defines greet(), app main calls greet(), [[dependencies]] name=util path=../util. Found on CRUSH-167.  _(panini-d, 2026-10-07)_
- [ ] **issue** — crush-pkg's dead-code lint flags a used path dep as unreferenced ('dependency util declared in [dependencies] but not referenced by entry file') when the entry calls util's functions by name — the lint looks for the dep's NAME in the entry, but Crush deps are concatenated, so their functions are called unqualified. False positive on every working dep; under --message-format=strict it would fail CI. Found on CRUSH-167.  _(panini-d, 2026-10-07)_
- [ ] **issue** — Cargo.lock on main still records workspace crates at 0.3.8 while the workspace version is 0.3.9: bump-version.sh bumps Cargo.toml but doesn't refresh Cargo.lock, so every cargo build in a fresh checkout rewrites 39 lock entries and leaves a dirty tree that agents must remember not to commit. bump-version should run 'cargo update -w' (or cargo metadata) and commit the lock with the bump.  _(panini-d, 2026-10-07)_
- [ ] **issue** — Ticket ID CRUSH-108 is used twice: tickets/CRUSH-108-jit-nan-eq-test-failing-main.md and CRUSH-108-stdlib-reconcile-source-and-dedupe.md. Only the stdlib one was closed by CRUSH-169; the JIT NaN one needs a renumber or explicit status so references to 'CRUSH-108' stay unambiguous.  _(panini-e, 2026-10-07)_
- [ ] **issue** — stdlib/README.md (top-level, polyglot transpiled modules) claims the generated CAST 'is loaded at compile time when a Crush program uses import <module>' — false: import lowers to an unregistered module.load cap call (see docs/design/import-system.md, CRUSH-110). Fix the README or land CRUSH-110.  _(panini-e, 2026-10-07)_
- [ ] **gap** — crates/crush-walker-core/README.md documents only the legacy tree-sitter Walker trait; its example uses walker_core:: and Program { statements, meta } and does not compile; links ../python_walker/README.md (missing). crush-lang-go/README links ../walker-core (dir is crush-walker-core). Point both at docs/design/walker-authoring.md.  _(panini-e, 2026-10-07)_
- [ ] **issue** — Walker extension tables are duplicated 4x (adapter file_extensions, aotc.rs load_casm_program, crates/cli walker_binary, crush-frontend WalkerRegistry) and PythonFrontend/Go bin list dotted extensions ('.py') that AdapterRegistry::can_handle never matches. Merge into one table.  _(panini-e, 2026-10-07)_
- [ ] **issue** — FastVM lowering (fastvm/instructions.rs) and AOT codegen (crush-aot codegen.rs/codegen_c.rs) match ai_toolchain/ai_goal_declaration/ai_knowledge_sharing but crush-frontend emits ai_tool_chain/ai_goal_decl/ai_knowledge_share, so those three AI ops never lower on FastVM/AOT (found CRUSH-156)  _(nimbus-b, 2026-10-07)_
- [ ] **gap** — FastVM AI ops (execution.rs) and AOT make_ai_stub don't pop the stack operands the frontend pushes for context_aware/semantic_match/synthesize, and AOT pushes a value for statement-form AI ops (goal/progress/knowledge) — stack imbalance vs CVM1 after CRUSH-156; FastVM can't service stack-operand kinds in resolve_host_request  _(nimbus-b, 2026-10-07)_
- [ ] **gap** — ai_semantic_switch / ai_capability_discovery / ai_adaptation_request still lower to NOP in crush-lang-sdk compile.rs; semantic_switch's compiled target is left on the stack and its case table is never consulted (found CRUSH-156)  _(nimbus-b, 2026-10-07)_
- [ ] **opportunity** — Crush text syntax has no AI expressions: crush-frontend's parser only produces AIStatement::SemanticSwitch (and CSON @synthesize → Synthesize); query/toolchain/delegation exist only via CAST producers, so .crush programs can't use the ai_native engine from CRUSH-156..158  _(nimbus-b, 2026-10-07)_
- [ ] **issue** — crush-lang-sdk doctor::tests::version_falls_back_to_stderr is flaky in CI (failed once on PR #118 Test (sdk), run 37813777655: probe_version returned None). Likely ETXTBSY: fake_tool writes a #!/bin/sh script then execs it while other test threads fork and briefly inherit the write fd. Fix: retry spawn on ETXTBSY in the test helper (or probe_version), or serialize the fake-tool tests.  _(panini-cap, 2026-10-08)_
- [ ] **issue** — crush-vm prefixes every HostCap error with 'unknown capability: <name>:' even when the cap IS registered and refused for a real reason (e.g. 'unknown capability: fs.cat: path escapes sandbox root: ../x'). Misleading: reads as a missing grant. Found running squad-bridge-peek (CRUSH-172) under crush-run --fs.  _(panini-cap, 2026-10-08)_
- [ ] **gap** — tree-sitter-crush has 0 Rust tests; its grammar corpus (crates/tree-sitter-crush/test/corpus) only runs under the tree-sitter CLI ('tree-sitter test'), which no CI job invokes. The grammar is never checked against its corpus in CI. Found during CRUSH-231.  _(naka, 2026-10-09)_
- [ ] **issue** — CVM1 ROT (scheduler.rs ~L624, portable_vm.rs ~L659) computes [x,y,z] -> [y,x,z] while FastVM/JIT Rot is [x,y,z] -> [y,z,x]; engines disagree on the same opcode. casm_to_vm now lowers casm 'rot' to ROLL 2 to dodge it, but a hand-written .casm using ROT still diverges.  _(panini, 2026-10-09)_
- [ ] **issue** — Frontend tuple/list/vector/set literals emit new_X, then per element 'dup; <elem>; X_push'. CVM1 *_PUSH pops container+value and re-pushes the container (tuples by value), so each element leaves a stray container on the stack. Now that casm_to_vm lowers these ops, verify a tuple/list/set literal end-to-end on CVM1 and fix the dup in compiler.rs (same bug the ArrayLiteral arm already fixed).  _(panini, 2026-10-09)_
- [ ] **gap** — casm_to_vm lowers dom_query/dom_mutate/dom_event_listener, ai_adaptation_request/ai_capability_discovery/ai_semantic_switch and export_var to NOP silently - code that uses them runs and does nothing. Decide per op: real lowering (assembler lacks DOM_* mnemonics though the opcodes exist) or an explicit Unsupported error.  _(panini, 2026-10-09)_
- [ ] **issue** — casm_to_vm emitted bare 'SPAWN' with no operand, so every program with spawn failed assembly (SPAWN takes 1 operand); frontend emits argc. Fixed in CRUSH-232; no prior test compiled a spawn through casm_to_vm.  _(panini, 2026-10-09)_
