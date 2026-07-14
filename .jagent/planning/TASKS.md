# TASKS — crush-ast

## P0 — Build & Core Health ✅

- [x] `--all-features` build fixed (rustls dep:)
- [x] `--no-default-features` build (crush-net needs cfg gates)
- [x] 129+ tests pass (11 Rust SDK + 8 Python SDK + 11 JS SDK + 12 C SDK + 99 VM + 6 walker), 0 failures
- [x] Core crates published (casm, crush-cast, crush-errors, crush-vm, crush-frontend, crush-lang-sdk)
- [x] **LTO enabled**: 3-layer (Rust fat LTO + gcc -flto + CFLAGS -flto). Binary size 64-80% reduction (53-142MB → 19-30MB)

## P1 — VM Coverage & Parity ✅

- [x] portable_vm parity: 10 missing opcodes added + 11 parity tests
- [x] Lambda compilation wired
- [x] Match compilation wired
- [x] async/await/spawn parsed
- [x] **bitwise opcode coverage** (bit_and/or/xor/not/shl/shr in AOT C + AOT Rust)
- [x] **math opcode coverage** (math_pow/sqrt/abs/round/floor/ceil in AOT C + AOT Rust)
- [x] **string opcode coverage** (str_contains/starts_with/ends_with/to_upper/to_lower/trim in AOT C + AOT Rust)
- [x] **test_ffi_gateway_cap** now passes (auto-build via build.rs)
- [x] **libcrush_vm.so** built as cdylib (19MB with LTO)
- [ ] Wire AI-native opcodes in crush-vm (Query, Synthesize, AgentDelegation, etc.)
- [ ] Wire spawn/await/yield to VM execution
- [ ] Fill 18 zero-coverage error paths
- [ ] Fix MOD sign bug between portable_vm and FastVM
- [ ] Add EXEC_LANG opcode to PortableVm

## P2 — Walkers & Frontends ✅

- [x] Rust walker (syn → CAST)
- [x] Python walker (PyO3)
- [x] JS/TS walker (swc + boa dual-backend)
- [x] Bash walker (tree-sitter)
- [x] Zsh walker (tree-sitter)
- [x] C/C++ walker
- [x] Go walker
- [x] Zig walker
- [x] Wasm walker
- [x] **C walker SDK** (c_to_cast, cast_to_casm, run_c, 12 tests)
- [x] **Python walker SDK** (run_python, 8 tests, slices, in/is, AOT path)
- [x] **JS walker SDK** (run_js, 11 tests, subscript fix, TS stripping, polyglot merge)
- [x] **Rust walker SDK** (run_rust, 11 tests, field access, array literals, closures)
- [x] **AOT C path for all 4 languages** (C, Python, JS/TS, Rust ��� `crush-aotc compile --emit c`)
- [x] **Polyglot merge** (Program::merge → JS + Python in one CASM → one .so)

## P3 — AOT Backends ✅

- [x] **AOT C backend** (852L): CASM → C99 + gcc -O3 -flto, arrays/objects/math/string/bitwise/SIMD
- [x] **AOT Rust backend** (590L): CASM → Rust + rustc, math/string/bitwise parity with C
- [x] **Forward declarations** for cross-function calls in C codegen
- [x] **Array pool bump** (ARRAY_DATA_CAP 1024 �� 65536 for sieve workloads)
- [x] **Type inference disabled** for non-crush sources (Value-mode locals)

## P4 — JIT

- [x] Phase 1: Skeleton (stack ops, arithmetic, logic, jumps, locals, 21 tests)
- [ ] Phase 2: Locals & Calls (function calls, store/load, CapCall, CallHost)
- [ ] Phase 3: Data & Caps (MakeList, MakeMap, Index, Len, arena)
- [ ] Phase 4: Exceptions (EnterTry, ExitTry, Throw)
- [ ] Phase 5: ExoLight integration
- [ ] Phase 6: Optimization passes
- [ ] Phase 7: AOT compilation

## P5 — Debugger

- [x] Breakpoint registry (file:line keyed, bytecode resolution)
- [x] REPL (break, delete, step, continue, list, status, quit, help)
- [x] VM integration (set_breakpoints, DebugBreak yield, is_halted)
- [x] VmDriver abstraction (PortableVmDriver + MockVmDriver)
- [x] NDJSON wire consumer
- [ ] Variable inspection (`print <var>`)
- [ ] Source → bytecode sourcemap (crush-frontend integration)
- [ ] Step-by-step state inspection

## P6 — Cross-Project

- [x] **C↔Crush FFI bridge**: plugin auto-build, test_ffi_gateway_cap passing, libcrush_vm.so
- [ ] Tier-3: Migrate surfer's in-tree Crush runtime → crush-ast
- [ ] Reconcile divergence with exosphere's in-tree crush

## 💡 Aspirational

- [ ] V8 fallback for dynamic JS (feature-gated, snapshot-based, DevTools)
- [ ] Node.js API compatibility shim (require('http') → CAP_CALL)
- [ ] Embedded RustPython VM lane
- [ ] Subprocess/CPython lane + three-way lane router
- [ ] `exo.*` capability modules
- [ ] Import firewall, fuel budgets, deterministic mode, snapshot/replay
- [ ] Unified capsule-aware GC + ML "GC policy brain"
- [x] **issue** — crush-cson: inline comments are NOT stripped inside values (kai, 2026-07-14)
- [x] **issue** — crush-cson: annotation properties split on ',' naively (kai, 2026-07-14)
- [x] **gap** — crush-cson: string escapes unsupported, no serializer (kai, 2026-07-14)
- [x] **gap** — crush-cson: CsonParseCap never registered (kai, 2026-07-14)
- [ ] **issue** — crush-jit: silently miscompiles ~55 of 86 FastOps, cranelift fuzz target disagrees (panini, 2026-07-14)
- [ ] **issue** — lambdas cannot be written in crush: pipe token collision (panini, 2026-07-14)
