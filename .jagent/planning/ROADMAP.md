# Crush Roadmap — M5+ (post-JIT, post-`crush` umbrella)

> Extends the milestone ladder defined in `TASKS.md` (M1-M4 + Publish + Aspirational)
> for the next major arc. Anchors to the project's three thesis documents:
>
> - `docs/design/ai-native-roadmap.md` — Crush is *for* AI writers
> - `docs/design/crush-catalyst.md` — write in your language, ship at C speed
> - `docs/design/crush-jit-backend.md` — Cranelift-native performance story
>
> and surfaces from `TODO.md` priorities, `TASKS.md` open items, `VISION.md`, and
> 24 existing `planning/tickets/CRUSH-N.md`. New ticket IDs (`CRUSH-27+`) are
> proposed here; **no new file is created in `planning/tickets/` for them yet** —
> each gets filed only after its milestone is scoped and dependencies confirmed,
> following the discipline in `planning/RULES.md` ("verify-before-fix + one
> worktree/branch per milestone + push at every phase boundary").
>
> Cross-links:
>
> - Identity: `.jagent/PROJECT.md`
> - Backlog: `.jagent/TODO.md`
> - Current milestone ladder: `.jagent/planning/TASKS.md`
> - Architectural memory: `.dejavue/decisions.md`, `.dejavue/timeline.jsonl`

---

## Phase ladder (current + proposed)

| Phase  | Title                                              | Done condition summary                                                                  | Status   |
| ------ | -------------------------------------------------- | -------------------------------------------------------------------------------------- | -------- |
| M1     | Correctness sweep                                  | All 12 CRUSH-N tickets (excluding CRUSH-1 wiring, CRUSH-24 retired) verified fixed     | Mostly ✓  |
| M2     | JIT completion (Phases 1-7)                        | All 84 `FastOp` lowered; AOT-from-JIT live; differential coverage w/ AOT C              | In progress |
| M3     | Debugger completion                                | Source-map + var print + step inspection                                               | Partial  |
| M4     | Cross-project integration                          | Surfer migration + exosphere reconcile                                                | Partial  |
| Publish | crates.io lane                                    | All 35 crates via `version.workspace`; `walker-core` published; taxon renames          | Open     |
| **M5** | **AI-native compiler layer**                       | Annotations as CAST nodes; `crush-index` live; AI opcodes execute                      | Proposed |
| **M6** | **Walker parity & multi-language completeness**    | 11 walker lowering gaps closed; LanguageAdapter unification; Java/Kotlin                | Proposed |
| **M7** | **Runtime hardening & ops tooling**                | Fuel + determinism + firewall + snapshot + V8/RustPython lanes                         | Proposed |
| **M8** | **Platform & architecture maturation (CRUSH-22)**  | CI multi-OS + multi-arch; Android API; `wasm32` first-class                              | Proposed |
| **M9** | **Cross-project convergence & STDLIB restoration** | Surfer migrated; exosphere/nakshatra converged; 103 caps restored; 46 rewritten        | Proposed |
| **M10** | **Performance ceiling & optimization**            | JIT miscompile closure; Phase 6/7 optimization; conservative→precise GC                | Proposed |
| **M11** | **Universal native & WASM catalyst**               | WASM walker→AOT; cross-language inlining; notebook self-hosting                         | Proposed |

---

## M5 — AI-native compiler layer

**Thesis.** Make contracts, invariants, and relationships first-class
compiler-checked CAST nodes — not free-text comments. Agents query them
(`codebase.*` host caps) and the compiler enforces their presence. Without this
layer, the "AI-native" promise documented in `ai-native-roadmap.md` is hollow.

**Done condition.**

- [ ] `@module`, `@invariants`, `@errors`, `@reads`, `@writes`, `@covers`,
      `@exhaustive-match-sites` are formal node types in `crush-cast`
- [ ] `crush-frontend` parser recognizes the annotations above
- [ ] `crush-frontend` compiler passes them through into CASM output
- [ ] `crush-index` crate v0 ingests CAST → SQLite-backed index (symbols, call
      graph, dependency graph, invariants, coverage map, exhaustive-match sites)
- [ ] `codebase.modules | invariants | uncovered_paths | exhaustive_sites |
      callers | semantic_search` host caps are wired into the default
      `crush-lang-sdk` runtime
- [ ] CRUSH-1 (10 AI-native opcodes + spawn/await/yield) wired into `crush-vm`
      execution (currently NOP at the VM level; AOT-side stubs **already merged**
      to `main` at `f49ece5` 2026-07-20 as part of the salvage from
      `agent/buffy/CRUSHAST-CRUSH-1` — VM-side wiring remains the gap, and is
      what's blocking `crush-notebook` AI-native cells)

**Proposed tickets** (`CRUSH-27+`):

- [CRUSH-27] `@module`/`@invariants`/`@errors`/`@reads`/`@writes`/`@covers` as
  CAST nodes (`crush-cast` types + parser/compiler emit) — Steps 1-3 of
  `ai-native-roadmap.md`
- [CRUSH-28] `crush-index` crate v0 — CAST → SQLite + JSON export (Step 4)
- [CRUSH-29] `codebase.*` host caps over `crush-index` (Steps 5-6)
- [CRUSH-30] `@exhaustive-match-sites` compiler lint (Step 7)
- [CRUSH-31] dejavue ↔ crush-index integration: change-feed joins (Step 8)
- [CRUSH-32] AI opcodes wire-up (CRUSH-1): `Query`, `Synthesize`,
  `AgentDelegation`, `SemanticMatch`, `LearningLoop`, `ContextAware`,
  `ToolChain`, `GoalDeclaration`, `ProgressUpdate`, `KnowledgeSharing`. **Scope
  after this ticket starts**: VM-execution side only. AOT-side stub emission
  for these opcodes is already merged at `f49ece5` 2026-07-20 (salvaged from
  the `CRUSHAST-CRUSH-1` workstream before that branch was retired); the
  remaining gap is the `crush-vm` execution tier (scheduler.rs +
  portable_vm.rs + FastVM and JIT lowering of these new opcodes).
- [CRUSH-33] DOM opcodes: `dom_mutate`, `dom_event_listener`, `dom_query`
- [CRUSH-34] `spawn`/`await`/`yield` to VM execution (three opcodes plus
  scheduling rules)

**File ownership.**

- `crush-cast` (annotation node types)
- `crush-frontend` (parser + compiler emit)
- `crush-index` (new crate)
- `crush-lang-sdk` (cap provider wiring)
- `crush-vm` (AI opcodes + spawn/await/yield)
- `crush-notebook` (consumer of AI cells — unblocks when CRUSH-32 done)

**Coupling.**

- Depends on: M1 (correctness sweep clean) — annotation nodes need a stable AST
- Blocks: M9 (exosphere-side annotation adoption for ARCHITECTURAL alignment

---

## M6 — Walker parity & multi-language completeness

**Thesis.** A user should be able to write in any of 12+ supported languages with
real algorithmic parity — not "works on arithmetic only." `VISION.md` is explicit
that this is the single highest-leverage work in the whole project right now.

**Done condition.**

- [ ] All **7 remaining** walker-lowering gaps from `VISION.md`'s Walker
      Lowering Progress table closed: `range()`/iteration, tuple unpacking,
      list comprehensions, slice expressions, `Math.floor()`, typed arrays
      (Uint8Array), generic function-call-with-args. (Per VISION.md's table as
      of s388: 5 ops are already ✅ closed — floor division `//`, strict
      equality `===`/`!==`, logical not `!`, `__crush_assign__`, and `print()`/
      `io.print` cap; the remaining 7 🔴 items are what this milestone closes.)
- [ ] Every published walker (`crush-lang-c | python | js | rust | go | bash |
      zsh | custom`) has a complete walker→AOT path (not just CVM1)
- [ ] Six legacy `Frontend`-trait-only crates (`bash`, `custom`, `nepali`,
      `python`, `rust`, `zsh`) migrated onto the unified `LanguageAdapter` trait;
      fixes `crates/cli/src/main.rs`'s `py`/`pyw` `→` `python_walker` mapping
      (which references a non-existent crate today)
- [ ] Java + Kotlin walkers landed (CRUSH-21 sub-tickets for language family);
      tree-sitter-java/tree-sitter-kotlin grammar integration
- [ ] WASM + Dart + Zig walker crates reach `1.0` (currently `0.1.0` per the
      Publish-lane version drift)

**Proposed tickets** (`CRUSH-35+`):

- [CRUSH-35] Walker-lowering completion (7 remaining gaps per `VISION.md`'s
  Walker Lowering Progress table; the 5 already-closed ops do not need
  re-work)
- [CRUSH-36] `LanguageAdapter` trait unification + 6-crate migration
- [CRUSH-37] `crush-lang-java` walker (tree-sitter-java)
- [CRUSH-38] `crush-lang-kotlin` walker (tree-sitter-kotlin)
- [CRUSH-39] Walker→AOT pipeline maturation for all 12 walkers (currently only
  c/python/basic-js have direct AOT paths)

**File ownership.**

- Each `crush-lang-*` crate
- `crates/cli` (CLI mapping registration)
- `docs/benchmarks/GAPS.md` (docs update; new entry per op gap-closing)

**Coupling.**

- Depends on: Partial M5 (Phase 1 of `@exhaustive-match-sites` lint for
  walker-statement lowering precision)
- Blocks: M11 (universal WASM pipeline cross-walks all 12 walkers;
  `crush-web` integration assumes walker parity)

---

## M7 — Runtime hardening & ops tooling

**Thesis.** The VM must be production-grade. Today several execution-tier
limits mean a stuck host call hangs forever (partially fixed by CRUSH-19), a
divergent JS execution buffers unlimited memory, a long-running agent has no
progress guarantees. This phase wires the production infrastructure
`ai-native-roadmap.md` and `VISION.md` both call out.

**Done condition.**

- [ ] CRUSH-19-style cooperative timeout extended to **all** blocking host
      caps (not just `CAP_CALL`; `IO_READ`, `IO_WRITE`, `NET_CONNECT`,
      `PROCESS_WAIT`, `HOST_REQUEST` etc.)
- [ ] Fuel budgets (default 1B instructions per program with per-instruction
      counter, JIT-equivalent tick) wired into `scheduler.rs`, `portable_vm.rs`,
      and `crush-jit`
- [ ] Deterministic mode: `HashMap`/`HashSet` → `BTreeMap`/`BTreeSet` for all
      state-touched data structures (program.functions, type registry, arena
      slot maps), behind `deterministic` cfg; reproducible `cargo test` output
- [ ] Import firewall: `crush-pkg` parses `import` declarations and rejects
      non-allowlisted upstream packages per program-manifest configuration;
      gate owned by `crush-lang-sdk` runtime host-cap provider
- [ ] Snapshot/replay: serialize VM state (mid-arena, mid-function-call) to a
      `.cvm-snapshot` blob; replay deterministically into PortableVM and FastVM;
      no JIT replay required
- [ ] V8 fallback path for dynamic JS (feature-gated `v8-fallback`;
      snapshot-based; DevTools attached)
- [ ] `require('http')` → `CAP_CALL` Node.js compatibility shim (subset only —
      not full Node.js coverage; document gaps in `docs/design/`)
- [ ] Embedded RustPython VM lane: `crush-lang-python` ships a `runtime =
      "rustpython"` option that has no host Python dependency, for
      hermetic-CI / sandboxed-polyglot use
- [ ] `exo.*` capability modules exposed: `exo.io`, `exo.fs`, `exo.process`,
      `exo.net`, `exo.env` (pass-through mediation to the existing
      `io.*` / `fs.*` / etc. caps with proper capability mediation rules)

**Proposed tickets** (`CRUSH-40+`):

- [CRUSH-40] Cooperative wall-clock timeout for **all** blocking caps (extension
  of CRUSH-19)
- [CRUSH-41] Fuel budgets (FastVM + scheduler + portable_vm enforcement)
- [CRUSH-42] Deterministic mode (HashMap→BTreeMap; build-cfg; CI invariant)
- [CRUSH-43] Import firewall (`crush-pkg` allowlist semantics)
- [CRUSH-44] Snapshot/replay spec + PortableVM/FastVM impl
- [CRUSH-45] V8 fallback feature (`v8-fallback`, behind cfg)
- [CRUSH-46] Node.js API compat shim (`require('http')` subset)
- [CRUSH-47] Embedded RustPython VM lane (`crush-lang-python` `runtime =
  "rustpython"` option)
- [CRUSH-48] `exo.*` capability module layer + mediation rules

**File ownership.**

- `crush-vm` (fuel, determinism, snapshot)
- `crush-pkg` (firewall)
- `crush-lang-sdk` (shim, exo modules, cap registration)
- `crush-lang-javascript` (V8 fallback)
- `crush-lang-python` (RustPython lane)

**Coupling.**

- Depends on: M2 fully complete (steady-state JIT for snapshot/replay
  benchmark)
- Blocks: M9 (exosphere convergence assumes deterministic-mode parity)

---

## M8 — Platform & architecture maturation (CRUSH-22 evolved)

**Thesis.** Today the project builds and tests on `ubuntu-latest` only. Two AOT
backends disagree on `target_os` cfg coverage; there is zero arch-specific
(`aarch64` / `riscv`) code anywhere. The next arc multiplies CI lanes and adds a
real surface for `aarch64`, `riscv`, mobile, and Windows.

**Done condition.**

- [ ] CI runs `ubuntu-latest` + `macos-latest` + `windows-latest` as the stable
      matrix (each runs the full `cargo test --workspace` + `cargo check
      --all-features` + the differential `crush-diff` harness)
- [ ] CI includes `aarch64-ubuntu` + `riscv64-ubuntu` cross-compile matrix
      lanes (with `cross`); AOT output is bit-for-bit comparable to x86_64 AOT
      on equivalent inputs
- [ ] `crush-aot`, `crush-aotc`, `crush-installer` (`main.rs:466`'s
      `#[cfg(target_os = "windows")]` branch), and `crush-ptx` reach consensus
      on `target_os` / `target_arch` cfg coverage — **all three** of the OS-cfg
      sites flagged in CRUSH-22 (`crush-aot`'s `.so`/`.dylib`/`.dll` branching
      in `compiler.rs`; `crush-aotc`'s unconditional `Command::new("cc")` in
      `codegen.rs`; `crush-installer`'s separate Windows branch) reconciled.
      (CRUSH-22's own text says "two AOT backends silently disagree" — the
      installer third-site is part of the same problem class and should be
      reconciled in this pass; not just AOT backends.)
- [ ] `wasm32-unknown-unknown` target is a first-class `crush-web` build (the
      crate already merged in the recent `crush-ptx`/`crush-web` work — verify
      + document, not rebuild)
- [ ] Pi-class build (`aarch64-unknown-linux-gnueabihf`) is the default
      install target for embedded use; `crush-installer` script augments its
      target-list
- [ ] Android API surface (CRUSH-21 platform shard): `crush-lang-android` host
      caps implemented; sample Crush app runs on Android emulator with
      end-to-end test

**Proposed tickets** (`CRUSH-49+`):

- [CRUSH-49] CI multi-OS matrix (.github/workflows)
- [CRUSH-50] CI multi-arch matrix (cross-compile lanes)
- [CRUSH-51] AOT `target_arch` cfg audit + reconciliation
- [CRUSH-52] Android API host cap shard (`crush-lang-android`)
- [CRUSH-53] `crush-installer` Pi-class default + boots-on-bare-metal
  smoke test

**File ownership.**

- `.github/workflows/*.yml` (CI matrix)
- `crush-aot`, `crush-aotc` (cfg audit)
- `crush-web` (verify wasm lane; not rebuild)
- `crush-installer`
- `crush-lang-android` (new crate)

**Coupling.**

- Depends on: M2 fully complete (JIT compiles for all targets) **and**
  **CRUSH-26 fixed first** — the existing `Build (release)` CI job has been
  failing workspace-wide for 24+ hours on `main` due to `ort-sys`'s missing
  `onnxruntime` static lib; adding new matrix lanes (CRUSH-49, CRUSH-50) while
  the existing matrix is red won't deliver signal. CRUSH-26 is the
  precondition for CRUSH-49/50 to be meaningful.
- Blocks: M11 (WASM target assumes `wasm32` lane stable + AOT cfg agreed)

---

## M9 — Cross-project convergence & STDLIB restoration

**Thesis.** For `crush-ast` to deliver on its "powers surfer / notebook /
exosphere / nakshatra" mission, the surrounding projects must converge — and
the archived capabilities in `exosphere-1.0.zip` must either restore cleanly
or be rewritten. **Restore silent corruption is worse than not restoring**;
every restored cap will be required to carry an M5 `@covers` test that proves
its behavior, not just a smoke test.

**Done condition.**

- [ ] Surfer's in-tree Crush runtime fully migrated to `crush-ast` (no dual
      maintenance; the exo-light migration path documented in
      `docs/design/exec-lang-pluggable-executor.md` is the canonical seam)
- [ ] Exosphere divergence reconciled (cross-tree `crush` modules merged via
      the schema-specific design owned by exo's `[main]/buffy` work)
- [ ] CRUSH-23: Nakshatra half finalized (no sandboxed Crush engine in
      nakshatra, but `tools/build.crush` artifact on exosphere's path is
      recorded as the canonical artifact; deferred to exosphere-side)
- [ ] **103** of 137 archived capabilities cleanly restored from
      `exosphere-1.0.zip` with **zero** mock markers (each passed through the
      M5 coverage map requirement so restoration is verified by `@covers`
      test, not by hand)
- [ ] **46** mock-tainted archived capabilities rewritten from spec (not
      verbatim-restored) under their own tickets, with spec provenance
      recorded in `dejavue decision`
- [ ] STDLIB RESTORATION MAP turned into a tracker (linked from chronicle
      ticket and from the M5-`codebase.modules` query)

**Proposed tickets** (`CRUSH-54+`):

- [CRUSH-54] Surfer migration wave 1: in-tree runtime → `crush-ast`
  (re-export, drop-in, no behavior change); wave 2: replace in-tree forks
- [CRUSH-55] Exosphere divergence reconcile (cross-tree)
- [CRUSH-56] STDLIB clean-restore tracker (103 caps; one CRUSH ticket per 10
  caps so granular blame stays tractable)
- [CRUSH-57] STDLIB mock-rewrite tracker (46 caps; one CRUSH ticket per cap —
  rewrites touch behavior, not just code reuse)
- [CRUSH-58] Nakshatra artifact canonicalization (companion to CRUSH-23)

**File ownership.**

Cross-project; tracked by foreman at workspace level:

- `surfer/` (parent repo)
- `exosphere/` (parent repo)
- `nakshatra/` (parent repo)
- `stdlib/` (in `crush-ast`)
- `.dejavue/decisions.md` (write the schema-provenance entries)

**Coupling.**

- Depends on: All of M5 + M6 + M7 (capability surface must be stable)
- Blocks: **1.0-release-candidate lane** — when M9 done, project is
  "release-candidate-ready"

---

## M10 — Performance ceiling & optimization

**Thesis.** The current implementation leaves 10–100× speedups on the table —
particularly around JIT miscompile fixes, GC policy, and compiler optimization
passes. This phase is about reaching the **steady-state performance ceiling**,
not incremental baseline improvements.

**Done condition.**

- [ ] `crush-jit`'s "55 of 86 FastOps miscompile" gap (the panini
      2026-07-14 finding) closed — each remaining op verified against AOT C
      reference under differential testing (`crush-diff` harness extension)
- [ ] JIT Phase 6 (Optimization passes): constant folding, dead-code
      elimination, inlining of small functions live; benchmark progress
      measured vs. AOT per workload
- [ ] JIT Phase 7 (AOT compilation from JIT): the JIT can dump its own
      compiled native code back as a `.so` for cold-start-free deployment
- [ ] Conservative → precise GC cutover (shadow stack → real stack maps) —
      eliminates GC pauses for long-lived programs
- [ ] ML "GC policy brain" PoC: a small on-device ML model trained on
      per-program allocation patterns proposes a heuristic selection
      between conservative / precise / regional GC

**Proposed tickets** (`CRUSH-59+`):

- [CRUSH-59] `crush-jit` FastOps audit closure (55/86 miscompile fix)
- [CRUSH-60] JIT Phase 6 (Optimization passes)
- [CRUSH-61] JIT Phase 7 (AOT-from-JIT dump)
- [CRUSH-62] Conservative → precise GC cutover
- [CRUSH-63] ML GC policy brain PoC

**File ownership.**

- `crush-jit` (audit + Phase 6/7)
- `crush-vm` (GC cutover)
- `docs/benchmarks/` (new entries per optimization milestone)

**Coupling.**

- Depends on: M2 complete (Phases 1-5 stable)
- Blocks: M11 (WASM/native benchmark parity assumes this ceiling)

---

## M11 — Universal native & WASM catalyst

**Thesis.** **Crush-as-the-Pipeline-Compiler**: every supported language
becomes a *usable* compiler input — combine Python + Rust + Zig + Bash into
one `.so` with cross-language inlining. WASM is the synthesis path that makes
the coupon universality work. `VISION.md` §7 (§"Cross-language inlining" §)
and the `crush-catalyst.md` "promise" both reach for this.

**Done condition.**

- [ ] `wasm_walker` (currently `crates/wasm_walker`) migrates into a new
      `crush-lang-wasm` crate, gains a full walker→AOT path, and benchmarks
      against AOT-C for native cargo / librsvg / emscripten output with
      parity within 2× on `nqueens`/`sieve`/`mergesort`
- [ ] Cross-language inlining across **two distinct** walker inputs verified
      end-to-end — Python → inlined JS function inside a C-codegen `.so`
      produces correct output
- [ ] Universal native compile CLI mode: `crush compile *.crush hello.py
      lib.rs build.sh main.zig --emit native` lands a single `.so`
- [ ] Self-hosting walker pipeline in `crush-notebook`: cells in any of
      12+ supported languages execute through the walker→AOT path **inside**
      the notebook kernel (no subprocess), sharing the same arena + variable
      scope across cells of different languages
- [ ] `crush-notebook` ships integration tests confirming state-sharing
      between cells of different languages works (the "`crush-notebook`
      Jupyter-killer" claim verifiable in CI)

**Tickets.**

To be designed during **M6 → M7 → M10** in flight; numbers TBD at milestone
closure (will land in `CRUSH-64+` range).

**File ownership.**

- `crush-lang-wasm` (promotion)
- `crush-aotc` (multi-input compilation mode)
- `crush-notebook` (kernel integration)

**Coupling.**

- Depends on: All of M5, M6, M10 **and M8 for `wasm32` lane stability**
  (M11's "WASM walker→AOT" item requires M8's `wasm32-unknown-unknown` lane
  to be first-class and exercised in CI, which is M8's done condition)
- Blocks: **Headline release** — this is what the project is *for*; without
  M11, the project is still "a compiler with fragments of a universal
  pipeline," not the catalyst-product-positioning from `crush-catalyst.md`

---

## Suggested ticket numbering convention going forward

| Band          | Scope                                                                  | Naming                  |
| ------------- | ---------------------------------------------------------------------- | ----------------------- |
| `1-26`        | Existing tickets (M1-M4)                                              | (don't renumber)        |
| `27-34`       | **M5** — AI-native compiler layer                                      | `[CRUSH-27+]`           |
| `35-39`       | **M6** — Walker parity & multi-language                                | `[CRUSH-35+]`           |
| `40-48`       | **M7** — Runtime hardening & ops                                       | `[CRUSH-40+]`           |
| `49-53`       | **M8** — Platform & architecture (CRUSH-22 evolved)                    | `[CRUSH-49+]`           |
| `54-58`       | **M9** — Cross-project convergence & STDLIB                            | `[CRUSH-54+]`           |
| `59-63`       | **M10** — Performance ceiling                                          | `[CRUSH-59+]`           |
| `64+`         | **M11** + future bands (M12 distributed runtimes, M13 static analysis) | `[CRUSH-64+]`           |

---

## Build-order summary

The diagram has been *reconciled* with the per-milestone "Coupling" sections
above (review caught a v1 inconsistency where the diagram had `M2 → M5` and
`M5 → M8` edges that the per-milestone coupling text did not actually
require — the v2 below is faithful to the Coupling sections).

```
M1 ──▶ M5   (annotations, index, AI opcodes; M5 needs M1's stable AST)
M2 ──▶ M7   (runtime hardening; M7 needs M2's stable JIT)
M5 ──▶ M6   (walker parity; M6 needs partial M5 @exhaustive-match-sites lint)
M2 ──▶ M10  (perf ceiling; M10 builds on M2's stable JIT)
M2 ──▶ M8   (platform lanes; M8 needs M2 compiles-for-all-targets)
M6 ──▶ M9   (cross-project convergence; M9 needs M6's walker parity for surfer)
M7 ──▶ M9   (cross-project convergence; M9 needs M7's deterministic mode)
M10 ──▶ M11 (universal native; M11 needs M10's perf ceiling)
M8 ──▶ M11  (universal native + WASM catalyst; M11 needs M8's wasm32 lane)
```

M9 takes its dep-list as a *conjunction* of `M5 + M6 + M7` (all three
required for "capability surface stable"). If surfer migration defers,
scope M9 down to "exosphere reconcile + STDLIB restoration" — note the M9
section above.

---

## Risks / open questions

- **M5 + CRUSH-1 (CRUSH-32) sequencing.** M5 is the better long-term
  foundation, but the AI-opcodes wire-up is a `crush-notebook` blocker today.
  Recommend parallel tracks (CRUSH-32 in M5-band; the annotation node-type
  spec, `crush-cast` work, is the intersection). Single-arc sequencing will
  starve `crush-notebook`.
- **M9 dependency on M6.** Surfer migration assumes walker parity because
  surfer embeds Crush for **Python** workloads heavily. If M6 stalls, surfer
  is stuck. Consider scoping down M9 to "exosphere reconcile + STDLIB
  restoration" if surfer migration can be deferred.
- **Import firewall placement (M7) — spec early to avoid M6/M7 collision.**
  The firewall is in M7 (runtime hardening), but `crush-pkg` parses `import`
  declarations and a future walker added in M6 may have a different surface
  syntax the firewall must accept. **Recommendation**: file a tiny
  CRUSH-NN ticket in the **M5 band** that specifies the firewall's
  `crush-pkg` parse hook shape only (no implementation), so M7 isn't
  blocked-on-design when M7 starts. Without this, M6 walker additions can
  silently regress the firewall contract.
- **M11 is the headline but takes longest.** Plan to ship individual
  milestones (M5-M10) as concrete release candidates along the way — don't
  hold the whole arc hostage to M11.
- **STDLIB restoration scale.** 103 + 46 = 149 caps. Treat as a separate
  workstream with its own tracker tickets (CRUSH-56, CRUSH-57). Restoring
  silent corruption is worse than not restoring; the M5 `@covers`
  requirement is the safety net, not a nice-to-have.
- **Ticket ID gaps (`CRUSH-3` `CRUSH-4` `CRUSH-5` `CRUSH-6`).** Twenty-six is
  the highest existing in `planning/tickets/`, but the IDs are not contiguous
  (3-6 missing; 17 used twice in two adjacent files). Verify-before-fix rule
  from `planning/RULES.md` still applies — ID gaps are likely retired/abandoned
  tickets, not free. Use `CRUSH-27+` going forward without back-filling.
- **Compile-time of `crush-lang-*` walker convergence at M6.** M2's 1m22s
  `cargo check -p crush-lang-sdk -p crush-aot` is a recent baseline; adding
  Java + Kotlin + improved Ruby in M6 → M6.5 should not push that past 3m in
  CI (kernel-cache hot). Flag if it does — fork crates selectively.

---

## What this doc is NOT

- **Not a replacement for `TASKS.md`.** `TASKS.md` is the canonical
  milestone ladder and ticketing tracker; integration into `TASKS.md` is a
  follow-up once the M5 ticket stubs are filed and the first M5 horse starts.
  Do **not** duplicate tickets between `TASKS.md` and this doc — the right
  path is to file the CRUSH-27+ tickets under
  `.jagent/planning/tickets/` and update `TASKS.md` M5-M11 sections in-place
  with one-line entries that link here.
- **Not a commitment.** Each CRUSH-27+ ticket listed here is a **proposed
  stub**, not a verified open item. Per the discipline in
  `planning/RULES.md`, tickets are filed only after dependencies are
  re-verified against current `main` at the time the work starts.
- **Not a schedule.** No dates are attached to any milestone. The existing
  session counter (`s388`, `s394`, etc. in `dejavue/timeline.jsonl`) is the
  authoritative progress signal.
- **Not a complete backlog.** The 6 uncovered opcodes (`BITAND`, `BITOR`,
  `BITXOR`, `BITNOT`, `SHL`, `SHR`), 18 zero-coverage error paths,
  5 uncovered capability functions, the MOD-sign-bug, the unreachable code
  at `vm.rs:326`, and EXEC_LANG-import gaps listed in `TODO.md`/`TASKS.md`'s
  Publish lane are small backlog items, not milestone-class work. They
  close in the Publish lane or as drive-by fixes during M2/M5, not as
  standalone M5–M11 milestones.
