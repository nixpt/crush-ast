# Roadmap — crush-ast

Living plan. Dejavue holds *why*; this file holds *sequence*.

## North star

A complete AI-native language ecosystem: parser, compiler, multi-tier VM (CVM1→FastVM→JIT), debugger, 9+ language walkers, CSON data format, and agent-native tooling — all shipping from a single workspace.

## Current phase: Production hardening & gap closure

The core pipeline (parser → CAST → CASM → CVM1/FastVM) is shipped and battle-tested (874 tests). The roadmap ahead is:

1. **Make AI opcodes real** — currently 10 AI opcodes + 3 DOM opcodes + spawn/await/yield = 16 NOPs
2. **Complete JIT** — Phases 2-7 of the 7-phase Cranelift roadmap
3. **Finish debugger** — variable inspection, sourcemap integration
4. **Fill test gaps** — 18 error paths with zero coverage

## Milestones

| Phase | Name | Goal | Exit criteria |
|-------|------|------|----------------|
| **M0** | Ship the core | Compiler + VM + REPL + 3 walkers working. | 500+ tests, 0 warnings, binaries ship. |
| **M1** | Full VM parity | All parsed features executable (no NOPs). AI opcodes wired. | 16 NOPs → 0 NOPs at runtime. |
| **M2** | JIT completion | Cranelift JIT full parity with FastVM. | All 84 FastOp instructions JIT-enabled. |
| **M3** | Debugger completion | Variable inspection, sourcemaps, step-by-step. | Full VSCode-compatible debugging. |
| **M4** | Cross-project integration | surfer crush runtime unified, exosphere reconciled. | No duplicate in-tree crush. |

## Non-goals (standing)

- **Web-hosted IDE** — crush is CLI-first; notebook is a separate project (crush-notebook)
- **WASM browser execution** — the VM runs natively; WASM sandboxing is for polyglot blocks
- **Full GC** — reference-counted arena today; GC is aspirational Phase 11
- **Self-hosting compiler** — crushc is Rust-based; self-hosting is aspirational Phase 10

## Version tags (when releasing)

| Tag | Maps to |
|-----|---------|
| v0.1.0 | M0 complete |
| v0.2.0 | M1 complete (current: partial — 16 NOPs remain) |
| v0.3.0 | M2 complete |
| v0.4.0 | M3 complete |
| v1.0.0 | M4 complete |
