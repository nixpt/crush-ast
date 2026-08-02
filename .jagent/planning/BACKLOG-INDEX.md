# BACKLOG-INDEX — the dispatch map (milestone → ticket → prompt)

> Authored s412 (2026-08-02) per captain's directive: horses build well but don't
> architect, so the architecture happens here, once — a large pre-authored backlog
> where every dispatch-ready ticket has a spec in `tickets/` and a ready-to-go
> prompt in `workspace-meta/prompts/crush-backlog/`. Foreman (or any captain
> session) picks the top unblocked ticket, launches the prompt, done.
>
> Statuses here are the DISPATCH view; `RULES.md` still applies — **verify the
> repro before fixing**. A ticket marked open below is a claim, not a fact.
> Sources: `ROADMAP.md` (M5–M11 specs), `research/2026-07-14-crush-ast-opportunities.md`
> (ranked findings), CRUSH-71 audit (in flight, panini), dejavue timeline captures.

## ID ledger

| Range | Allocation |
|-------|-----------|
| 1–26  | M1-era correctness sweep + platform tickets (mostly done; open: 1, 23, 26) |
| 17    | ⚠ COLLISION — two tickets share the ID; parser-errors one is DONE (s388), jit-phase-2-4 one renumbered → **CRUSH-87** |
| 27–34 | M5 AI-native layer (filed; 31, 33 done — the duplicate older spec files are superseded) |
| 35–58 | M6–M9 (ROADMAP-proposed; only 36–38 filed so far) |
| 59–63 | M10 (ROADMAP-proposed, unfiled) |
| 64    | reserved-free (ROADMAP's stale "M11 = 64+" claim; M11 actually lands at 98+) |
| 65–70 | s403–s411-era bug tickets (filed) |
| 71    | Design audit + client survey campaign (panini persona-session, IN FLIGHT s412) |
| 72–78 | NEW: research-doc ranked findings (see below) |
| 79–86 | NEW: CRUSH-71 opening-survey captures (see below) |
| 87    | NEW: renumbered jit-phase-2-4 correctness gaps (ex-CRUSH-17 collision) |
| 88–97 | NEW: M9 STDLIB clean-restore shards (10 tickets × ~10 caps, per CRUSH-56's own design) |
| 98–102| NEW: M11 universal-native/WASM tickets |
| 103+  | free |

## How to dispatch from this index

1. Pick the topmost ticket whose **Gate** column is clear (or whose gate is met).
2. Its prompt lives at `workspace-meta/prompts/crush-backlog/CRUSH-NN.txt`
   (absolute: `/home/nixp/WORKSPACE/workspace-meta/prompts/crush-backlog/`).
   Prompts are pre-linted (`dispatch-prompt lint --strict`).
3. Launch per persona-session or foreman-dispatch-wave. Preferred identity for
   compiler-lane tickets: `panini`; VM-runtime lane: `nimbus`; fastvm: `buffy`;
   bindings/build: `sangam`. Lane boundaries are in each prompt.
4. Every prompt requires incremental commits (the panini s390 lesson) and
   carries halt-criteria + the nimbus-contract flag where relevant.

## P0 — build health (dispatch before anything else)

| ID | Title | Status | Gate | Prompt |
|----|-------|--------|------|--------|
| CRUSH-26 | CI `Build (release)` ort-sys/onnxruntime failure | open | — | crush-backlog/CRUSH-26.txt |

## In flight

| ID | Title | Who | Status |
|----|-------|-----|--------|
| CRUSH-71 | Design audit + client survey + top-win implementation | panini (persona-session s412) | branch `agent/panini-crush/CRUSH-71` |

## Correctness spine (research-doc ranked findings — NEW, 72–78)

The 2026-07-14 meta-finding: crush-ast repeatedly builds both ends of a feature,
never connects the middle, and has no test that would notice. These tickets ARE
the spine; most other work is gated on 73/77 existing first.

| ID | Title | Rank | Effort | Gate | Prompt |
|----|-------|------|--------|------|--------|
| CRUSH-72 | crush-jit: silent TAG_NULL catch-all → Unsupported error + FastVm fallback | #1 ⚠ ships wrong answers today | hours | — | crush-backlog/CRUSH-72.txt |
| CRUSH-73 | Conformance corpus: expectation-annotated `.crush` files + one black-box runner across all four engines | #2 — the mechanism that stops the meta-finding recurring | days | — | crush-backlog/CRUSH-73.txt |
| CRUSH-74 | Wire source locations through AST (parser keeps `SourceLocation`, meta carries line/col into casm debug_info) | #3 | days | — | crush-backlog/CRUSH-74.txt |
| CRUSH-75 | Lambda syntax unreachable: lexer's bare-`\|`-as-Ident shortcut vs parser's Token::Pipe expectation | #4 | days | — | crush-backlog/CRUSH-75.txt |
| CRUSH-76 | Fuzz the parser (cargo-fuzz targets: lexer, parser, cson) | #5 | days | — | crush-backlog/CRUSH-76.txt |
| CRUSH-77 | Differential harness: same program → PortableVm vs FastVm vs JIT vs AOT-C, assert equal (extend `crush-diff` to AOT) | #6 | days | CRUSH-73 helps but not required | crush-backlog/CRUSH-77.txt |
| CRUSH-78 | Memory-model decision: PortableVm leaks cycles; the mark-sweep GC lives on the other value model — decide + wire or delete | #7 | design-first | — | crush-backlog/CRUSH-78.txt |

## CRUSH-71 opening-survey captures (NEW, 79–86)

Filed from panini's dejavue captures (commit `a5a24c8`, 2026-08-02). Cross-repo
tickets stay filed here (crush-ast is the ecosystem anchor) but their prompts
dispatch into the client repo.

| ID | Title | Kind | Repo | Prompt |
|----|-------|------|------|--------|
| CRUSH-79 | casm DebugInfo.source_map flat-vector: wrong function's location for multi-function programs | correctness | crush-ast | crush-backlog/CRUSH-79.txt |
| CRUSH-80 | casm dead code: CachedProgram/to_cached (O(F²), never wired) + ecasm.rs (1039 lines, 0 refs) — wire or delete | hygiene | crush-ast | crush-backlog/CRUSH-80.txt |
| CRUSH-81 | SemanticAnalyzer 4–14 full walks → Tarjan SCC reverse-topological inference | design/perf | crush-ast | crush-backlog/CRUSH-81.txt |
| CRUSH-82 | Lexer: Vec<char> whole-source copy + String per token → byte-span tokens + interner | design/perf | crush-ast | crush-backlog/CRUSH-82.txt |
| CRUSH-83 | Compile cache / incremental unit: content-hash casm cache + per-function memoization | design/perf | crush-ast | crush-backlog/CRUSH-83.txt |
| CRUSH-84 | crush-notebook casm_to_assembly maps unknown opcodes → NOP silently; needs hard-error arm | correctness | crush-workspace/crush-notebook | crush-backlog/CRUSH-84.txt |
| CRUSH-85 | exo-light fabric_executor fakes exit_code:0 success when crush-run binary missing | correctness | openko-network/openko | crush-backlog/CRUSH-85.txt |
| CRUSH-86 | Dead crush-vm deps: squeeze (zero usage) + crush-visuals-debug-bridge (only uses crush_debugger) | hygiene | crush-workspace | crush-backlog/CRUSH-86.txt |

## M2 — JIT completion (existing lane)

| ID | Title | Status | Gate | Prompt |
|----|-------|--------|------|--------|
| CRUSH-87 | JIT Phase 2–4 correctness gaps (float Mod, serr checks, handler_pc contract, StoreLocal audit, call-stack overflow) — ex-CRUSH-17 collision, renumbered | open | CRUSH-72 first (stop the silent-null bleeding) | crush-backlog/CRUSH-87.txt |
| — | Phases 3–7 tracked at milestone level in TASKS.md; Phase-6/7 tickets are CRUSH-60/61 (M10) | | | |

## M5 — AI-native compiler layer (filed 27–34)

| ID | Title | Status | Gate | Prompt |
|----|-------|--------|------|--------|
| CRUSH-27 | Annotation CAST node types | triage pending | — | crush-backlog/CRUSH-27.txt |
| CRUSH-28 | crush-index v0 (CAST → SQLite) | triage pending | CRUSH-27 | crush-backlog/CRUSH-28.txt |
| CRUSH-29 | `codebase.*` host caps | triage pending | CRUSH-28 | crush-backlog/CRUSH-29.txt |
| CRUSH-30 | `@exhaustive-match-sites` lint | triage pending | CRUSH-27 | crush-backlog/CRUSH-30.txt |
| CRUSH-31 | dejavue ↔ crush-index integration | DONE (file says so; dup spec file superseded) | | |
| CRUSH-32 | AI opcodes VM wire-up (CRUSH-1's execution half) | triage pending | — | crush-backlog/CRUSH-32.txt |
| CRUSH-33 | DOM opcodes VM wire-up | DONE (all 3 commits landed; dup proposal file superseded) | | |
| CRUSH-34 | spawn/await/yield VM execution | triage pending (two spec files — reconcile) | M2 scheduler | crush-backlog/CRUSH-34.txt |

## M6 — Walker parity (35–39)

| ID | Title | Status | Gate | Prompt |
|----|-------|--------|------|--------|
| CRUSH-35 | Walker-lowering completion (7 remaining VISION.md gaps) | unfiled → mint | — | crush-backlog/CRUSH-35.txt |
| CRUSH-36 | LanguageAdapter unification + 6-crate migration | filed | — | crush-backlog/CRUSH-36.txt |
| CRUSH-37 | crush-lang-java walker | filed | CRUSH-36 | crush-backlog/CRUSH-37.txt |
| CRUSH-38 | crush-lang-kotlin walker | filed | CRUSH-37 | crush-backlog/CRUSH-38.txt |
| CRUSH-39 | Walker→AOT maturation for all 12 walkers | unfiled → mint | CRUSH-35 | crush-backlog/CRUSH-39.txt |

## M7 — Runtime hardening (40–48, all unfiled → mint)

| ID | Title | Gate |
|----|-------|------|
| CRUSH-40 | Cooperative timeout for ALL blocking caps (CRUSH-19 extension) | — |
| CRUSH-41 | Fuel budgets (scheduler + portable_vm + JIT tick) | — |
| CRUSH-42 | Deterministic mode (HashMap→BTreeMap behind cfg) | — |
| CRUSH-43 | Import firewall (crush-pkg allowlist) | spec-early risk flagged in ROADMAP |
| CRUSH-44 | Snapshot/replay (.cvm-snapshot, PortableVM+FastVM) | CRUSH-42 |
| CRUSH-45 | V8 fallback feature | — |
| CRUSH-46 | Node.js compat shim (require('http') subset) | — |
| CRUSH-47 | Embedded RustPython lane | — |
| CRUSH-48 | exo.* capability module layer | — |

## M8 — Platform maturation (49–53, unfiled → mint; GATED on CRUSH-26)

| ID | Title | Gate |
|----|-------|------|
| CRUSH-49 | CI multi-OS matrix | **CRUSH-26** |
| CRUSH-50 | CI multi-arch matrix (cross) | CRUSH-49 |
| CRUSH-51 | AOT target_arch cfg audit (3 sites) | — |
| CRUSH-52 | Android API host cap shard | — |
| CRUSH-53 | crush-installer Pi-class default | — |

## M9 — Convergence + STDLIB (54–58 + shards 88–97)

| ID | Title | Gate |
|----|-------|------|
| CRUSH-54 | Surfer migration waves 1+2 | M5–M7 surface stable |
| CRUSH-55 | Exosphere divergence reconcile | coordinate w/ exo [main]/buffy |
| CRUSH-56 | STDLIB clean-restore tracker (meta-ticket over 88–97) | M5 @covers gate |
| CRUSH-57 | STDLIB mock-rewrite tracker (46 caps; individual tickets minted on demand, NOT pre-minted — rewrites need per-cap spec provenance) | CRUSH-56 |
| CRUSH-58 | Nakshatra artifact canonicalization | — |
| CRUSH-88..97 | Clean-restore shards 1–10 (~10 caps each, zero mock markers, @covers-verified) | CRUSH-56 spec |

## M10 — Performance ceiling (59–63, unfiled → mint)

| ID | Title | Gate |
|----|-------|------|
| CRUSH-59 | JIT FastOps audit closure (the 55/86 completion; CRUSH-72 is the correctness stopgap that comes first) | CRUSH-72, CRUSH-77 |
| CRUSH-60 | JIT Phase 6 optimization passes | CRUSH-59 |
| CRUSH-61 | JIT Phase 7 AOT-from-JIT | CRUSH-60 |
| CRUSH-62 | Conservative → precise GC cutover | CRUSH-78 decision |
| CRUSH-63 | ML GC policy brain PoC | CRUSH-62 |

## M11 — Universal native + WASM (98–102, unfiled → mint)

| ID | Title | Gate |
|----|-------|------|
| CRUSH-98 | wasm_walker → crush-lang-wasm promotion + walker→AOT path | M8 wasm32 lane |
| CRUSH-99 | Cross-language inlining PoC (Python→JS inside C-codegen .so) | CRUSH-98 |
| CRUSH-100 | Universal native compile CLI mode | CRUSH-99 |
| CRUSH-101 | Notebook self-hosting walker pipeline (in-kernel, no subprocess) | CRUSH-98 |
| CRUSH-102 | Notebook cross-language state-sharing CI tests | CRUSH-101 |

## Publish lane

| ID | Title | Gate |
|----|-------|------|
| (mint on demand) | version.workspace sweep + walker-core publish + `walker`→`crush-walker` rename | after M6's trait unification (CRUSH-36) settles crate boundaries |

## Open questions for CRUSH-71's audit to settle

- Final ranking of 79–83 (the design/perf captures) against 72–78: the audit's
  measured baseline decides what actually gates "3000x"-class wins.
- Whether the compile-cache ticket (CRUSH-83) subsumes casm's dead CachedProgram
  (CRUSH-80) or replaces it.
- Client-survey matrix may add tickets beyond 84–86.
