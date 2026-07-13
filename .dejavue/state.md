# State — crush-ast

Updated: 2026-07-13T03:25:00-05:00
Version: v0.2.0

## Delivery snapshot

| Track | Status | Notes |
|-------|--------|--------|
| Core compiler pipeline | **shipped** | Parser(2,881L) → CAST → Semantics → Optimizer(460L) → Compiler(1,884L) → CASM |
| CVM1 (PortableVm) | **shipped** | 40+ opcodes, debugger-aware, step/breakpoint/continue |
| FastVM | **shipped** | 84 FastOp instructions, lowered bytecode, hot-path interpreter |
| crush-jit (Cranelift) | **partial** | Phase 1: stack ops, arithmetic, logic, jumps, locals (21 tests). 7-phase roadmap. |
| AI-native expressions | **stub** | 7 expression types + 3 statement types. All 10 compile to NOP. |
| Async/spawn/await | **stub** | Parsed and stored in AST. Compile to NOP. |
| Annotations | **shipped** | @wip, @temporary, @decision, @invariant, @errors, @covers. W-WIP-001 + W-TMP-001 warnings. |
| Debugger | **partial** | Breakpoints, step/continue, REPL. Variable inspection returns "not yet implemented" (todo!()). |
| CLI tools | **shipped** | crushc, crush-run, crush-repl, crush-debugger, crush-pkg, crush-installer, walker |
| Multi-frontend walkers | **shipped** | Rust, Python, JS/TS, Bash, Zsh, C/C++, Go, Zig, Wasm |
| CSON | **shipped** | CSON→CAST desugaring. Weights, fuzzy keys, @annotations. |
| Package ecosystem | **shipped** | crush-pkg (build/lint/site/extract), crush-installer, crush-python, crush-net, crush-index |
| Cross-project notebook | **shipped** | crush-notebook uses crush-frontend + CVM1 for cell evaluation |

## Metrics

| Metric | Value |
|--------|--------|
| Crates | 35 + xtask |
| Tests | 874 pass, 1 fail (known: FFI plugin needs .so), 6 ignored |
| Warnings | 0 |
| NOP-at-runtime opcodes | 16 (10 AI + 3 DOM + spawn + yield + await) |
| VM backends | 3 (CVM1, FastVM, JIT Phase 1) |
| Walker frontends | 9 |
| CLI binaries | 9 |
| Decisions captured | 21 |

## Active arcs

_None._ Full-session audit complete (2026-07-13). State updated.

## Blockers

_None._ Single test failure is pre-existing and non-blocking (requires compiled C plugin).

## Next (suggested)

1. Wire AI-native opcodes (unblocks crush-notebook M2)
2. Wire spawn/await/yield to VM
3. Complete debugger variable inspection
4. Advance JIT to Phase 2
5. Fill 18 zero-coverage error path tests
