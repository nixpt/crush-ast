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
example programs — all 9 now **fully resolved** via this M1 session:

| Ticket | Status | Fix |
|--------|--------|-----|
| CRUSH-7 (array mutation) | ✅ fixed | index-assignment, chained .push(), scheduler/portable return array |
| CRUSH-8 (stale examples) | ✅ fixed | recursive type inference (Null→Any in BinaryOp), for-loop continue target |
| CRUSH-9 (JS-walker types) | ✅ fixed | lenient Null handling in BinaryOp, Any compatibility in merge_types |
| CRUSH-11 (AOT C strings) | ✅ fixed | ring-buffer append in _add, _str_dup in store, str_to_upper/lower/trim |
| CRUSH-12 (struct kills main) | ✅ fixed | already fixed by unrelated prior work, re-verified |
| CRUSH-13 (5-way arithmetic) | ✅ fixed | div/mod-by-zero, overflow, mixed-type comparisons unified across all backends |
| CRUSH-14 (io.print newline) | ✅ fixed | trailing newline in scheduler.rs and portable_vm.rs |
| CRUSH-15 (CASM dialect) | ✅ works | text round-trip verified (basic, strings, function calls, recursion) |
| CRUSH-10 (AOT Rust backend) | ✅ fixed | already fixed by unrelated prior work, re-verified |

Next milestone: CRUSH-1 (AI opcodes) — wire 10 AI-native opcodes + spawn/await/yield
 to real VM execution (currently all NOP). Then M2 (JIT completion), M3 (debugger).

Two previously-"P0-critical" tickets (CRUSH-2 polyglot capability bypass,
CRUSH-10 AOT-Rust backend) were re-verified s388 and are **already fixed** by
unrelated work — don't assume a ticket's Backlog status is current, see
`RULES.md` §1.

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

## Next items (M1 completed, onward to M2/M3)

1. **CRUSH-1** (L): Wire 10 AI-native opcodes + spawn/await/yield to real VM execution (currently all NOP). Largest remaining single item.
2. **M2 — JIT completion**: Phases 2-7 (Locals & Calls, Data & Caps, Exceptions, ExoLight, Optimizations, AOT).
3. **M3 — Debugger completion**: Variable inspection, sourcemap, step-by-step state.
4. **Publish lane**: Version drift, walker-core publishing, crate name fix.
5. **STDLIB RESTORATION MAP**: 103 of 137 archived capabilities clean/restorable.

## Memory split

| Concern | Path |
|---------|------|
| *Why* | `.dejavue/` (`dejavue context`) |
| *What / when* | `.jagent/planning/` (this file, ROADMAP, TASKS, tickets) |
| *How to work this backlog* | `.jagent/planning/RULES.md` (worktree/branch/verify discipline) |
| Identity | `.jagent/PROJECT.md` |
| Active threads | `.dejavue/threads.md` |
