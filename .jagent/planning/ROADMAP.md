# Roadmap — crush-ast

Living plan. Dejavue holds *why*; this file holds *sequence*.

## North star

A complete AI-native language ecosystem: parser, compiler, multi-tier VM (CVM1→FastVM→JIT→AOT C→AOT Rust), debugger, 9+ language walkers, polyglot compilation (one binary from many languages), CSON data format, and agent-native tooling — all shipping from a single workspace.

## Current phase: Correctness sweep (M1) — black-box bugs found by actually running programs

The core pipeline and polyglot AOT compilation are shipped. Two "critical/P0"
tickets in this very folder (polyglot capability bypass, AOT-Rust backend not
compiling) turned out to already be fixed by unrelated work when re-verified
s388 (2026-07-16) — **re-run every repro before fixing it**, per `RULES.md` §1.
The roadmap ahead:

1. **M1 correctness sweep** — 9 real bugs found by porting actual example programs
   (array mutation, JS-walker type inference, struct-declaration-kills-main,
   5-way arithmetic divergence, and smaller gaps). See TASKS.md's M1 section.
2. **Make AI opcodes real** — currently 10 AI opcodes + 3 DOM opcodes + spawn/await/yield = 16 NOPs
3. **Complete JIT** — Phases 2-7 of the 7-phase Cranelift roadmap
4. **Finish debugger** — variable inspection, sourcemap integration
5. **Publish lane** — version drift + walker-core publish, unblocks the crush-lang-* crates.io release

## Milestones

| Phase | Name | Goal | Exit criteria |
|-------|------|------|----------------|
| **M0** | Ship the core | Compiler + VM + REPL + 3 walkers working. | 500+ tests, 0 warnings, binaries ship. ✅ |
| **M1** | Correctness sweep + full VM parity | Black-box bugs (CRUSH-1/7/8/9/11/12/13/14/15) fixed; AI opcodes wired. | 0 known silent-failure bugs; 16 NOPs → 0 NOPs at runtime. |
| **M2** | JIT completion | Cranelift JIT full parity with FastVM. | All 84 FastOp instructions JIT-enabled. |
| **M3** | Debugger completion | Variable inspection, sourcemaps, step-by-step. | Full VSCode-compatible debugging. |
| **M4** | Cross-project integration | surfer crush runtime unified, exosphere reconciled. | No duplicate in-tree crush. |
| **M5** | Polyglot AOT (shipped!) | C, Python, JS/TS, Rust → AOT C `.so`. Multi-file merge. | 4 languages, 1 binary. ✅ |
| **M6** | LTO + optimization | 3-layer LTO (Rust + gcc + C deps). 64-80% size reduction. | Release binaries 19-30MB. ✅ |

Each milestone (and each individually-sized ticket within M1) gets its own
worktree + branch, merged and pushed on completion — see `RULES.md` §2.

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
| v0.5.0 | M5 + M6 complete |
| v1.0.0 | M4 complete |
