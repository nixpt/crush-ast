# BACKLOG-INDEX — the dispatch map (milestone → ticket → prompt)

> Authored s412 (2026-08-02) per captain's directive: horses build well but don't
> architect, so the architecture happens here, once — a large pre-authored backlog
> where every dispatch-ready ticket has a spec in `tickets/` and a ready-to-go
> prompt in `workspace-meta/prompts/crush-backlog/`. Foreman (or any captain
> session) picks the top unblocked ticket, launches the prompt, done.
>
> Statuses verified s412 by a full triage (every ticket file cross-checked against
> `git log --all`, TASKS.md, and the actual code — statuses below are relative to
> HEAD `060d9c5`). `RULES.md` still applies at dispatch time: **verify the repro
> before fixing.**
> Sources: `ROADMAP.md` (M5–M11 specs), `research/2026-07-14-crush-ast-opportunities.md`
> (ranked findings), CRUSH-71 audit (in flight, panini), dejavue timeline captures.

## ID ledger

| Range | Allocation |
|-------|-----------|
| 1–26  | M1-era sweep + platform (done except: 1 near-closeable, 11 verify+close, 21 partially, 22→M8, 23→M4/58) |
| 17    | ⚠ was a COLLISION — parser-errors ticket owns the ID (Done s388); the jit-phase-2-4 file renumbered → **CRUSH-87** |
| 27–34 | M5 (27/28/29/31/32/33 DONE on HEAD; 30 partial-ambiguous; 34 in progress). 31/33/34 each had a superseded s394 spec stub — marked Superseded, kept per RULES |
| 35–58 | M6–M9 (36–38 filed; rest minted s412) |
| 39    | ⚠ ID BURNED — branch `agent/panini-crush/CRUSH-39` holds unrelated Math.* WIP stash; walker→AOT renumbered → **CRUSH-103** |
| 59–63 | M10 (minted s412) |
| 64    | reserved-free (ROADMAP's stale "M11 = 64+" claim) |
| 65–70 | s403–s411 bug tickets (65/67/68/69/70 done; 66 impl-on-branch, gated) |
| 71    | Design audit + client survey campaign (panini persona-session, IN FLIGHT s412) |
| 72–78 | Correctness spine: research-doc ranked findings (minted s412) |
| 79–86 | CRUSH-71 opening-survey captures (minted s412) |
| 87    | JIT Phase 2–4 residue (renumbered ex-17; mostly Closed — item #7 + Cranelift GVN/LICM note at crush-jit/src/lib.rs:2452) |
| 88–97 | M9 STDLIB clean-restore shards (minted s412) |
| 98–102| M11 (minted s412; note crates/crush-lang-wasm already exists — 98 is half-done) |
| 103   | Walker→AOT pipeline (ex-39, renumbered) |
| 104   | Publish lane (version.workspace sweep + walker-core publish + `walker`→`crush-walker` rename) |
| 105   | JVM/Android guest bridge (the unfiled CRUSH-21 sub-shard; gates CRUSH-52) |
| 106   | Typed OpCode emission (audit finding #1 — 4–6 allocs/instruction via serde_json) |
| 107   | CAST meta → packed Span (audit finding #2 — contract-coordinated; reshapes CRUSH-74's mechanism) |
| 108   | Reconcile CRUSH-56's restoration source + dedupe against already-wired `crush-lang-sdk` stdcaps (found during `awesome-crush` exploration, s439) |
| 109   | Disambiguate top-level `stdlib/` (polyglot transpiled modules) from CRUSH-56/57's native HostCap restoration work — includes an already-observed string-function naming overlap |
| 110+  | free (CRUSH-57's per-cap rewrite tickets mint here) |

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

## Verify+close quick wins (cheap dispatches, run the repro → flip status)

| ID | What to verify | Note |
|----|----------------|------|
| CRUSH-11 | AOT-C string garbling repro (`--emit c` + ctypes) | ring-buffer fixes on HEAD; ticket says "needs re-verification" |
| CRUSH-1 | AI opcodes end-to-end | close automatically when CRUSH-34 commits 2–3 land |
| CRUSH-30 | whether E-EXH-001 wildcard-warn satisfies the ticket | `missing_arms` is a documented stub pending a type registry — needs a scope ruling, then either close or re-scope |
| CRUSH-35 | VISION.md walker table refresh | 6/7 gaps verified closed in code; table still shows all red |

## In flight

| ID | Title | Who | Status |
|----|-------|-----|--------|
| CRUSH-71 | Design audit + client survey + top-win implementation | panini (persona-session s412) | branch `agent/panini-crush/CRUSH-71` |
| CRUSH-66 | `@lang[pypi:/npm:]` deps via buckets | impl exists on `agent/cece/CRUSH-66` (80c48f5), unmerged | GATED on buckets#4 (BUCKETS-15) |

## Correctness spine (72–78) — dispatch these first

The 2026-07-14 meta-finding: crush-ast repeatedly builds both ends of a feature,
never connects the middle, and has no test that would notice. These tickets ARE
the spine; most later work is gated on 73/77 existing.

| ID | Title | Rank | Gate | Prompt |
|----|-------|------|------|--------|
| CRUSH-72 | crush-jit silent TAG_NULL catch-all → Unsupported + FastVm fallback | #1 (spec re-verifies vs post-M2 state) | — | crush-backlog/CRUSH-72.txt |
| CRUSH-73 | Conformance corpus + one black-box runner across all four engines | #2 — meta-finding killer | — | crush-backlog/CRUSH-73.txt |
| CRUSH-74 | Source locations wired through AST → casm debug_info | #3 | pairs with CRUSH-79 | crush-backlog/CRUSH-74.txt |
| CRUSH-75 | Lambda syntax unreachable (lexer bare-`\|` shortcut) + lex-error on unknown operator chars | #4 | before CRUSH-82 | crush-backlog/CRUSH-75.txt |
| CRUSH-76 | Parser/lexer/cson fuzz targets | #5 | — | crush-backlog/CRUSH-76.txt |
| CRUSH-77 | Differential harness: all four engines, assert identical | #6 | extends existing crush-diff | crush-backlog/CRUSH-77.txt |
| CRUSH-78 | Memory-model decision (design-first) | #7 | gates CRUSH-62 | crush-backlog/CRUSH-78.txt |

## CRUSH-71 opening-survey captures (79–86)

| ID | Title | Kind | Repo | Prompt |
|----|-------|------|------|--------|
| CRUSH-79 | casm source_map flat-vector: wrong location for multi-fn programs | correctness | crush-ast | crush-backlog/CRUSH-79.txt |
| CRUSH-80 | casm dead code: CachedProgram (O(F²), unwired) + ecasm.rs — wire or delete | hygiene | crush-ast | crush-backlog/CRUSH-80.txt |
| CRUSH-81 | ✅ DONE — landed via CRUSH-71 (`11f7a1c` + seed-fix `97bd7c4`; 3.4–3.9x on chain shapes) | design/perf | crush-ast | — |
| CRUSH-106 | Typed OpCode emission — kill serde_json on the emit path (audit #1) | design/perf | crush-ast | crush-backlog/CRUSH-106.txt |
| CRUSH-107 | CAST meta HashMap → packed Span + side table (audit #2; nimbus/visuals contract — coordinate; pairs with CRUSH-74) | design/perf | crush-ast | crush-backlog/CRUSH-107.txt |
| CRUSH-82 | Lexer: byte-span tokens + interner | design/perf | crush-ast | crush-backlog/CRUSH-82.txt |
| CRUSH-83 | Compile cache / incremental unit (content-hash casm cache) | design/perf | crush-ast | crush-backlog/CRUSH-83.txt |
| CRUSH-84 | notebook casm_to_assembly unknown-opcode → NOP silently | correctness | crush-workspace/crush-notebook | crush-backlog/CRUSH-84.txt |
| CRUSH-85 | exo-light fabric_executor fakes exit 0 when crush-run missing | correctness | openko-network/openko | crush-backlog/CRUSH-85.txt |
| CRUSH-86 | Dead crush-vm deps: squeeze + crush-visuals-debug-bridge | hygiene | crush-workspace | crush-backlog/CRUSH-86.txt |

## M2 — JIT completion

| ID | Title | Status | Gate | Prompt |
|----|-------|--------|------|--------|
| CRUSH-87 | JIT Phase 2–4 residue (item #7 + unsolved Cranelift GVN/LICM, lib.rs:2452) — ex-17 | open (residue only) | CRUSH-72 | crush-backlog/CRUSH-87.txt |
| — | Phases 3–5 tracked in TASKS.md; Phase 6/7 = CRUSH-60/61 (M10) | | | |

## M5 — AI-native compiler layer (NEARLY COMPLETE)

| ID | Title | Status |
|----|-------|--------|
| CRUSH-27 annotation nodes · 28 crush-index v0 · 29 codebase.* caps · 31 dejavue join · 32 AI opcodes · 33 DOM opcodes | | ALL DONE on HEAD (verified s412) |
| CRUSH-30 | @exhaustive-match-sites lint | partial — see quick wins |
| CRUSH-34 | spawn/await/yield wire-up | in progress — Commits 2 (5-tier wiring) + 3 (differential fixture) remain → prompt crush-backlog/CRUSH-34.txt |

## M6 — Walker parity

| ID | Title | Status | Gate | Prompt |
|----|-------|--------|------|--------|
| CRUSH-35 | Walker-lowering: residual = typed arrays (Uint8Array) + VISION.md table refresh (6/7 verified closed) | open | — | crush-backlog/CRUSH-35.txt |
| CRUSH-36 | LanguageAdapter unification | partial — Sub-Commit 3 + CLI py/pyw fix remain | — | crush-backlog/CRUSH-36.txt |
| CRUSH-37 | Java walker | partial — skeleton landed, real parsing remains | CRUSH-36 | crush-backlog/CRUSH-37.txt |
| CRUSH-38 | Kotlin walker | open (no crate yet) | CRUSH-37 | crush-backlog/CRUSH-38.txt |
| CRUSH-103 | Walker→AOT for all 12 walkers (ex-39) | open | CRUSH-35 | crush-backlog/CRUSH-103.txt |

## M7 — Runtime hardening (40–48)

| ID | Title | Gate |
|----|-------|------|
| CRUSH-40 | Timeout for remaining blocking caps (CAP_CALL + EXEC_LANG already covered) | — |
| CRUSH-41 | Fuel budgets VM-side (JIT already has fuel; extend Quotas surface) | — |
| CRUSH-42 | Deterministic mode (also de-flakes CRUSH-77) | — |
| CRUSH-43 | Import firewall (spec placement first) | — |
| CRUSH-44 | Snapshot/replay | CRUSH-42 |
| CRUSH-45 | V8 fallback (off-by-default, build-weight analysis required) | — |
| CRUSH-46 | Node.js compat shim | — |
| CRUSH-47 | Embedded RustPython lane | — |
| CRUSH-48 | exo.* capability modules | — |

## M8 — Platform (49–53) — GATE CLEARED (CRUSH-26 verified done s412)

| ID | Title | Gate |
|----|-------|------|
| CRUSH-49 | CI multi-OS matrix (mind the warm-cache trap, CRUSH-CI-CACHE-1) | — |
| CRUSH-50 | CI multi-arch matrix | CRUSH-49 |
| CRUSH-51 | AOT target cfg audit (3 sites) | — |
| CRUSH-52 | Android host cap shard | CRUSH-105 (JVM bridge) |
| CRUSH-53 | Installer Pi-class default | — |

## M9 — Convergence + STDLIB (54–58 + shards 88–97)

| ID | Title | Gate |
|----|-------|------|
| CRUSH-54 | Surfer migration waves 1+2 | M5–M7 surface stable |
| CRUSH-55 | Exosphere divergence reconcile | coordinate w/ exo [main]/buffy + EXO-194 |
| CRUSH-56 | STDLIB clean-restore tracker (meta over 88–97; step 1 = locate/recreate the RESTORATION MAP) | CRUSH-31 join (done) |
| CRUSH-57 | STDLIB mock-rewrite tracker (46 caps, minted on demand) | CRUSH-56 |
| CRUSH-58 | Nakshatra artifact canonicalization | — |
| CRUSH-88..97 | Clean-restore shards 1–10 | CRUSH-56 |

## M10 — Performance ceiling (59–63)

| ID | Title | Gate |
|----|-------|------|
| CRUSH-59 | JIT FastOps closure (current counts re-verified in spec; July's 31/86 has moved) | CRUSH-72, CRUSH-77 |
| CRUSH-60 | JIT Phase 6 optimization passes (GVN refs at lib.rs:2371+ are an UNSOLVED workaround note, not a pass) | CRUSH-59 |
| CRUSH-61 | AOT-from-JIT .so dump (⚠ "M2 Phase 7" commits 9c4d2d5/52c1e07 are a DIFFERENT Phase 7) | CRUSH-60 |
| CRUSH-62 | Conservative→precise GC cutover | CRUSH-78 decision |
| CRUSH-63 | ML GC policy brain PoC (aspirational) | CRUSH-62 |

## M11 — Universal native + WASM (98–102)

| ID | Title | Gate |
|----|-------|------|
| CRUSH-98 | crush-lang-wasm walker→AOT (crate already exists — half-done) | M8 wasm32 lane, CRUSH-103 |
| CRUSH-99 | Cross-language inlining PoC | CRUSH-98 |
| CRUSH-100 | Universal native compile CLI | CRUSH-99 |
| CRUSH-101 | Notebook self-hosting pipeline | CRUSH-98 |
| CRUSH-102 | Notebook cross-language state-sharing CI | CRUSH-101 |

## Publish lane

| ID | Title | Gate |
|----|-------|------|
| CRUSH-104 | version.workspace sweep + walker-core publish + `walker`→`crush-walker` rename | CRUSH-36 |

## CRUSH-71 audit fold-in (s412 — settled)

- Audit ranking (§3.1): #1 serde_json instructions → **CRUSH-106**; #2 empty
  meta HashMap → **CRUSH-107** (and CRUSH-74 should implement spans AS 107's
  packed Span, not by stamping meta); #3 SemanticAnalyzer multi-pass → landed
  (CRUSH-81 ✅ via CRUSH-71, 3.4–3.9x). Perf dispatch order: 106 → 107(+74/79)
  → 83 (compile cache) → 82 (lexer).
- CRUSH-83 vs CRUSH-80: still open — 83's design doc decides; 80 defaults to
  delete.
- Client matrix confirmed 84–86 and added per-client canary/policy tickets in
  the client repos themselves (VISUALS-7/8/9, NB-025..029, SQUEEZE-7, LSP-1/2,
  VSC-1/2, GUIDE-1/2, WEB-1, polydex O-05).
- Blast-radius ranking (§2) for any casm/cast/value/frontend change: notebook
  casm mapping > visuals cast matches > value-enum split (razor vs bozo) >
  frontend facade — check the matrix before landing breaking changes.
