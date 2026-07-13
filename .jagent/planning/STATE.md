# Planning state — crush-ast

**Updated:** 2026-07-13T03:20:00-05:00
**Milestone focus:** Production hardening & JIT completion
**Branch:** `main`

## Delivery snapshot

| Track | Status | Notes |
|-------|--------|--------|
| Core compiler pipeline | **shipped** | Parser(2,881L) → CAST → Semantics → Optimizer(460L) → Compiler(1,884L) → CASM |
| CVM1 PortableVm | **shipped** | 40+ opcodes, debugger-aware, step/breakpoint/continue. Latest parity pass added 10 opcodes. |
| FastVM | **shipped** | 84 FastOp instructions, lowered bytecode, hot-path interpreter |
| crush-jit (Cranelift) | **partial** | Phase 1: stack ops, arithmetic, logic, jumps, locals. 21 tests. 7-phase roadmap. |
| AI-native expressions | **stub** | 7 expression types + 3 statement types defined in CAST. All compile to NOP at runtime. |
| Async/spawn/await/yield | **stub** | Parsed and stored in AST. All compile to NOP at runtime. |
| Annotations | **shipped** | @wip, @temporary, @decision, @invariant, @errors, @covers. W-WIP-001 and W-TMP-001 warnings. |
| Debugger | **partial** | Breakpoints, step/continue, REPL work. Variable inspection returns "not yet implemented". |
| CLI tools | **shipped** | crushc, crush-run, crush-repl, crush-debugger, crush-pkg, crush-installer, walker |
| Multi-frontend walkers | **shipped** | Rust(syn), Python(PyO3), JS/TS(swc+boa), Bash(tree-sitter), Zsh, C/C++, Go, Zig, Wasm |
| CSON | **shipped** | CSON→CAST desugaring. Semantic weights, fuzzy keys, @annotations. |
| Package ecosystem | **shipped** | crush-pkg (build/lint/site/extract), crush-installer, crush-python, crush-net, crush-index |
| Dejavue | **shipped** | Context, 20+ decisions, invariants, patterns, threads, state, timeline |
| Jagent planning | **shipped** | This file (just initialized), ROADMAP, TASKS, tickets |

## Active work

_Gap closure._ The pipeline is solid; focus shifts to (1) making AI opcodes real, (2) completing JIT, (3) test coverage.

## Blockers

_None urgent._ The single test failure (`crush-vm::tests::surfaces::test_ffi_gateway_cap`) requires an external compiled .so — not blocking.

## Metrics

| Metric | Value |
|--------|--------|
| Total crates | 35 (+ xtask) |
| Tests passed | 874 |
| Tests failed | 1 (known pre-existing: FFI plugin requires .so) |
| Tests ignored | 6 |
| Warnings | 0 |
| Language feature count | 25+ parsed, 18+ executable |
| AI opcodes | 10 defined, 0 executable |
| NOP-at-runtime opcodes | 16 (10 AI + 3 DOM + spawn + yield + await) |
| VM backends | 3 (CVM1, FastVM, JIT) |
| Walker frontends | 9 |
| CLI binaries | 9 |
| Build time (from clean) | ~120s |
| Decisions captured | 20+ |
| Known error-path gaps | 18 with zero coverage |

## Next 6 (from TASKS.md, priority order)

1. Wire AI-native opcodes in crush-vm (Query, Synthesize, AgentDelegation, etc.)
2. Wire spawn/await/yield to VM execution
3. Complete debugger variable inspection
4. Advance JIT to Phase 2 (function calls, cap calls, store/load)
5. Fill 18 zero-coverage error path tests
6. Add EXEC_LANG opcode to PortableVm

## Memory split

| Concern | Path |
|---------|------|
| *Why* | `.dejavue/` (`dejavue context`) |
| *What / when* | `.jagent/planning/` (this file, ROADMAP, TASKS, tickets) |
| Identity | `.jagent/PROJECT.md` |
| Active threads | `.dejavue/threads.md` |
