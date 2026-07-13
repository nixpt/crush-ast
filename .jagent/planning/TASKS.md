# TASKS — crush-ast

## P0 — Build & Core Health ✅

- [x] `--all-features` build fixed (rustls dep:<name>)
- [x] `--no-default-features` build (crush-net needs cfg gates — known)
- [x] 874 tests pass, 0 warnings
- [x] Core crates published (casm, crush-cast, crush-errors, crush-vm, crush-frontend, crush-lang-sdk)

## P1 — VM Coverage & Parity ✅

- [x] portable_vm parity: 10 missing opcodes added + 11 parity tests
- [x] Lambda compilation wired
- [x] Match compilation wired
- [x] async/await/spawn parsed
- [ ] Wire AI-native opcodes in crush-vm (Query, Synthesize, AgentDelegation, SemanticMatch, LearningLoop, ContextAware, ToolChain)
- [ ] Wire spawn/await/yield to VM execution
- [ ] Fill 18 zero-coverage error paths
- [ ] Cover 6 uncovered opcodes (BITAND, BITOR, BITXOR, BITNOT, SHL, SHR)
- [ ] Cover 5 uncovered capability functions (str.contains, str.split, str.replace, str.join, make_range)
- [ ] Fix MOD sign bug between portable_vm and FastVM
- [ ] Remove unreachable code in vm.rs:326
- [ ] Add EXEC_LANG opcode to PortableVm

## P2 — Walkers & Frontends ✅

- [x] Rust walker (syn �� CAST)
- [x] Python walker (PyO3)
- [x] JS/TS walker (swc + boa dual-backend)
- [x] Bash walker (tree-sitter)
- [x] Zsh walker (tree-sitter)
- [x] C/C++ walker
- [x] Go walker
- [x] Zig walker
- [x] Wasm walker

## P3 — JIT

- [x] Phase 1: Skeleton (stack ops, arithmetic, logic, jumps, locals, 21 tests)
- [ ] Phase 2: Locals & Calls (function calls, store/load, CapCall, CallHost)
- [ ] Phase 3: Data & Caps (MakeList, MakeMap, Index, Len, arena)
- [ ] Phase 4: Exceptions (EnterTry, ExitTry, Throw)
- [ ] Phase 5: ExoLight integration
- [ ] Phase 6: Optimization passes
- [ ] Phase 7: AOT compilation

## P4 — Debugger

- [x] Breakpoint registry (file:line keyed, bytecode resolution)
- [x] REPL (break, delete, step, continue, list, status, quit, help)
- [x] VM integration (set_breakpoints, DebugBreak yield, is_halted)
- [x] VmDriver abstraction (PortableVmDriver + MockVmDriver)
- [x] NDJSON wire consumer
- [ ] Variable inspection (`print <var>`)
- [ ] Source → bytecode sourcemap (crush-frontend integration)
- [ ] Step-by-step state inspection

## P5 — Cross-Project

- [ ] Tier-3: Migrate surfer's in-tree Crush runtime → crush-ast
- [ ] Reconcile divergence with exosphere's in-tree crush

## 💡 Aspirational

- [ ] Embedded RustPython VM lane
- [ ] Subprocess/CPython lane + three-way lane router
- [ ] `exo.*` capability modules
- [ ] Import firewall, fuel budgets, deterministic mode, snapshot/replay
- [ ] Unified capsule-aware GC + ML "GC policy brain"
