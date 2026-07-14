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
- [x] **issue** — crush-cson: inline comments are NOT stripped inside values. 'port: 8080  # the port' parses as String("8080  # the port") instead of Number(8080). The format advertises #/// comments (skip_whitespace_and_comments handles them BETWEEN tokens) but parse_value's unquoted branch reads to end-of-line without stripping them. Silent wrong type, no error.  _(kai, 2026-07-14)_
- [x] **issue** — crush-cson: annotation properties are split on ',' naively, so any comma inside a value truncates it AND silently drops the remainder. '@module { purpose: "parse a, b, c" }' yields purpose="parse a". Silent data loss.  _(kai, 2026-07-14)_
- [x] **gap** — crush-cson: string escapes unsupported — 'msg: "he said \"hi\""' is a hard parse error ('Missing colon in kv pair'). Also: duplicate keys silently last-win, errors carry no line/col, and there is no serializer (parse-only, cannot emit CSON).  _(kai, 2026-07-14)_
- [x] **gap** — crush-cson: CsonParseCap (vm_cap.rs, 58 LOC) exposes a 'cson.parse' host capability to the Crush VM but is NEVER registered anywhere — cson.parse does not exist at VM runtime. Either wire it up or delete it.  _(kai, 2026-07-14)_
- [ ] **issue** — crush-jit SILENTLY MISCOMPILES: crates/crush-jit/src/compiler.rs:421 ends its opcode match with '_ => { push(TAG_NULL) }'. The JIT implements ~31 of ~86 FastOps, so PushStr, Call, and every array/map/string/capability opcode compile to a silent null and execution CONTINUES — the same program can return a different, wrong answer under the JIT than the interpreter, with no error. Every surveyed impl does the opposite (keep the correct path; let the fast path DECLINE). Cranelift — our own backend — ships a fuzz target whose whole job is asserting JIT and interpreter agree. Fix is hours. Found s380 by panini (SQ-RESEARCH-BYOX), pinned to HEAD e1d5595.  _(panini, 2026-07-14)_
- [ ] **issue** — LAMBDAS CANNOT BE WRITTEN IN CRUSH: lexer.rs:700-710 maps a bare '|' to Token::Ident("|") (comment: 'Single | as ident for now') while the lambda parser expects Token::Pipe, which is '|>'. So the only lambda syntax the real parser accepts collides with the pipe operator. Meanwhile Expression::Lambda exists in the AST, compiler.rs:1588 compiles it, tree-sitter has five lambda rules, and test_lambda.crush documents |a,b| {...} as THE syntax. Verified empirically against a clean HEAD clone: '|a, b| { return a + b; }' fails to parse; a plain fn compiles fine. A 'for now' shortcut silently disabled a documented feature and our two front-ends disagree about the language. Found s380 by panini.  _(panini, 2026-07-14)_
