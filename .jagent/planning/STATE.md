# Planning state — crush-ast

**Updated:** 2026-07-16T03:00:00-05:00
**Milestone focus:** M1 — correctness sweep (see TASKS.md, RULES.md for the worktree/branch discipline)
**Branch:** `main` (8 branches merged s388: CAPTIMEOUT-1, EXECLANG-PLUGGABLE-1, BUCKETSPIKE-1/2, PTX-REBASE-1, WEB-1, COLLECTIONS-RECOVER, PYLOWER-1, SNAKE-1)

## Delivery snapshot

| Track | Status | Notes |
|-------|--------|--------|
| Core compiler pipeline | **shipped** | Parser(2,881L) → CAST → Semantics → Optimizer(460L) → Compiler(1,884L) → CASM |
| CVM1 PortableVm | **shipped** | 40+ opcodes, debugger-aware, step/breakpoint/continue. Latest parity pass added 10 opcodes. |
| FastVM | **shipped** | 84 FastOp instructions, lowered bytecode, hot-path interpreter |
| crush-jit (Cranelift) | **partial** | Phase 1: stack ops, arithmetic, logic, jumps, locals. 21 tests. 7-phase roadmap. |
| **AOT C backend** | **shipped** | CASM → C99 source → gcc/clang -O3 -flto → .so. Full opcode coverage: arrays, objects, math, string, bitwise, SIMD. 852L. |
| **AOT Rust backend** | **shipped** | CASM → Rust source → rustc → .so. Math, string, bitwise op parity with C codegen. 590L. |
| **Polyglot walker pipeline** | **shipped** | C → CVM1/AOT ✅, Python → CVM1/AOT ✅, JS/TS → CVM1/AOT ✅, Rust → CVM1/AOT ✅ |
| **C↔Crush FFI bridge** | **shipped** | Plugin auto-build, `test_ffi_gateway_cap` passing, `libcrush_vm.so` cdylib (19MB), C embed test |
| **Python bridge** | **shipped** | SDK (8 tests), slices, in/is operators, AOT path |
| **JS bridge** | **shipped** | SDK (11 tests), subscript fix, AOT path, TypeScript stripping, polyglot merge test |
| **Rust bridge** | **shipped** | SDK (11 tests), AOT path, field access, array literals, closures, codegen parity |
| **LTO configuration** | **shipped** | 3-layer: Rust fat LTO (release profile), gcc -flto (AOT C), CFLAGS -flto (tree-sitter deps). Binary size: 53-142MB → 19-30MB (64-80% reduction) |
| AI-native expressions | **stub** | 7 expression types + 3 statement types defined in CAST. All compile to NOP at runtime. |
| Async/spawn/await/yield | **stub** | Parsed and stored in AST. All compile to NOP at runtime. |
| Annotations | **shipped** | @wip, @temporary, @decision, @invariant, @errors, @covers. W-WIP-001 and W-TMP-001 warnings. |
| Debugger | **partial** | Breakpoints, step/continue, REPL work. Variable inspection returns "not yet implemented". |
| CLI tools | **shipped** | crushc, crush-run, crush-repl, crush-debugger, crush-pkg, crush-installer, walker |
| Multi-frontend walkers | **shipped** | Rust(syn), Python(PyO3), JS/TS(swc+boa), Bash(tree-sitter), Zsh, C/C++, Go, Zig, Wasm |
| CSON | **shipped** | CSON→CAST desugaring. Semantic weights, fuzzy keys, @annotations. |
| Package ecosystem | **shipped** | crush-pkg (build/lint/site/extract), crush-installer, crush-python, crush-net, crush-index |
| Dejavue | **shipped** | Context, 20+ decisions, invariants, patterns, threads, state, timeline |
| Jagent planning | **shipped** | STATE, ROADMAP, TASKS, tickets |

## Active work

M1 correctness sweep: 9 real bugs found by black-box testing against actual
example programs (array mutation, JS-walker type inference, struct-kills-main,
5-way arithmetic divergence, AOT string-output re-verification, plus smaller
gaps) — see TASKS.md's M1 section and `.jagent/planning/tickets/CRUSH-{1,7,8,9,11,12,13,14,15}`.
Two previously-"P0-critical" tickets (CRUSH-2 polyglot capability bypass,
CRUSH-10 AOT-Rust backend) were re-verified s388 and are **already fixed** by
unrelated work — don't assume a ticket's Backlog status is current, see
`RULES.md` §1. After M1: AI opcodes (CRUSH-1), JIT completion (M2), debugger (M3).

## Blockers

_None known._ `test_ffi_gateway_cap` passes (auto-build via build.rs).
CRUSH-16 (cargo test --workspace link failure, AOT-bins-under-LTO +
crush-python cdylib/rlib) has a known, scoped fix, just not yet applied.

## Metrics

| Metric | Value |
|--------|--------|
| Total crates | 35 (+ xtask) |
| Tests passed | **129+** (11 Rust SDK + 8 Python SDK + 11 JS SDK + 99 VM + 12 C SDK + 6 walker) |
| Tests failed | 0 (crush-lang-c tree-sitter link issue, pre-existing) |
| Tests ignored | 6 |
| Warnings | 0 |
| Language feature count | 25+ parsed, 18+ executable |
| AI opcodes | 10 defined, 0 executable |
| NOP-at-runtime opcodes | 16 (10 AI + 3 DOM + spawn + yield + await) |
| **VM backends** | **4 (CVM1, FastVM, AOT C, AOT Rust)** ← up from 3 |
| **JIT backends** | **2 (Cranelift partial, x86-64 via AOT C)** |
| Walker frontends | 9 |
| **Polyglot AOT languages** | **4 (C, Python, JS/TS, Rust)** ← new |
| CLI binaries | 9 |
| Build time (from clean) | ~120s (debug), ~2min (release+LTO) |
| **Release binary size** | **19-30MB** (down from 53-142MB) ← LTO |
| Decisions captured | 20+ |
| Known error-path gaps | 18 with zero coverage |

## Next 6 (from TASKS.md M1, priority order — verify-before-fix per RULES.md §1)

1. CRUSH-12: struct declaration silently kills `main` (purest silent-failure bug)
2. CRUSH-13: five arithmetic implementations disagree on div/mod-by-zero
3. CRUSH-7: array mutation effectively unusable (index-assign, chained push, nested indexing)
4. CRUSH-9: JS-walker type-inference bugs (non-local, order-dependent)
5. CRUSH-11: re-verify AOT-C string garbling against the real repro (turtle_runner.js) before fixing anything
6. CRUSH-1: wire AI-native opcodes (largest single item, L effort)

## Memory split

| Concern | Path |
|---------|------|
| *Why* | `.dejavue/` (`dejavue context`) |
| *What / when* | `.jagent/planning/` (this file, ROADMAP, TASKS, tickets) |
| *How to work this backlog* | `.jagent/planning/RULES.md` (worktree/branch/verify discipline) |
| Identity | `.jagent/PROJECT.md` |
| Active threads | `.dejavue/threads.md` |
