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
- [ ] **issue** — example.crush (crush-website playground) does NOT run: @python/@javascript/@rust polyglot blocks have NO runtime capability. 'crush-run caps' lists io/str/fs/env/time/bus/task/akg/process/crypto/graphics/stdlib/net/db — no polyglot cap exists. @python block yields 'Undefined variable: result' (no marshaling back); @javascript/@rust yield malformed-JSON 'expected quoted string' (half-wired). Polyglot is the language's headline pitch and the website's hero claim.  _(unknown, 2026-07-14)_
- [ ] **issue** — Parser rejects @io.print(...) in expression position ('Unexpected token in expression: AtIdent(io)') even though io.print IS a real built-in portable capability at runtime. The guide teaches @io.print in getting-started.md and appendix/quick_reference.md, but its own 82 examples use bare print(). Runtime has the cap; surface syntax cannot reach it.  _(unknown, 2026-07-14)_
- [ ] **gap** — Polyglot is UNWIRED, not absent — 3 distinct defects. (1) crush-lang-python crate exists but crush-lang-sdk (builds crush-run) has no dep on it; registration is a global side-effect registry (register_executor), so @python no-ops -> 'Undefined variable: result'. (2) @javascript HAS a builtin executor (builtin_executors.rs: js/javascript/es6/ecmascript) but dies earlier at CASM parse: 'expected quoted string, got {"code":...' — the block's JSON spec is embedded in CASM unescaped. (3) @rust has no executor at all. Nothing is feature-gated.  _(unknown, 2026-07-14)_
- [ ] **issue** — Version drift blocks the whole crush-lang-* publish lane. [workspace.package] version=0.2.0, but only 9 crates use version.workspace=true. 21 hardcode 0.2.0 (correct today, will silently drift on next bump). 6 hardcode 0.1.0 and HAVE drifted: walker-core, cli(pkg name=walker), go_walker, zig_walker, dart_walker, wasm_walker. walker-core@0.1.0 is a dep of 10 crates at 0.2.0 — incl crush-aot and ALL 8 crush-lang-* (bash/c/custom/js/nepali/python/rust/zsh). walker-core is NOT on crates.io, so none of those can publish. The 7 crates that ARE published (casm, crush-cast, crush-errors, crush-frontend, crush-lang-sdk, crush-vm, tree-sitter-crush @0.2.0) are clean of walkers — that's why they made it. Fix: version.workspace=true everywhere + publish walker-core. Note crates/cli pkg 'walker' is TAKEN on crates.io (passcod) — rename to crush-walker on unmerged branch agent/kai/CRUSHAST-RENAME.  _(unknown, 2026-07-14)_
- [ ] **issue** — The crush-lang-* vs *_walker naming split IS the polyglot bug, not a cosmetic issue. walker-core defines THREE traits for one job (source->CAST): Frontend (old, parse->Box<dyn Any>), Walker (tree-sitter specific), LanguageAdapter (the unifier, added by 'Universal Walker Adapter' commit). AdapterRegistry ONLY accepts Box<dyn LanguageAdapter>. The unification is INCOMPLETE: 6 crates still implement ONLY Frontend (crush-lang-{bash,custom,nepali,python,rust,zsh}) so they CANNOT be registered. The 4 *_walker crates implement LanguageAdapter. js+c straddle both. So crush-lang-* = old Frontend generation, *_walker = new LanguageAdapter generation. KICKER: crates/cli/src/main.rs:20 maps 'py'|'pyw' => Some("python_walker") — a crate that DOES NOT EXIST. The only 'python' registration in the tree is a MockAdapter in a walker-core unit test. That is precisely why @python silently no-ops. Fix = migrate the 6 Frontend-only crates onto LanguageAdapter AND unify the names; the rename forces the migration.  _(unknown, 2026-07-14)_
- [ ] **issue** — PRE-EXISTING on feat/python-crush-bridge (NOT caused by the rename or polyglot work): 'cargo test --workspace' fails exit 101 with E0308 'multiple different versions of crate casm in the dependency graph' in crush-python. crush-python is crate-type=[cdylib,rlib]; cargo emits 'output filename collision' warnings naming the SAME package twice (libcrush_vm.rlib/.so). Per-crate tests are green (crush-vm 128, crush-lang-sdk 100) which masks it — only the full workspace link fails. Verified against a clean target dir and against the base branch with the IDENTICAL command.  _(kai, 2026-07-14)_
- [ ] **issue** — @ is OVERLOADED in Crush across 4 constructs, all sharing one AtIdent token (lexer.rs:125 '// @mcp, @cap, @lang, etc'); the PARSER disambiguates by context, not the sigil. (1) polyglot blocks @python{}/@javascript{} — required. (2) compiler/backend directives @gpu/@kernel/@target — crush-ptx lane. (3) AST/AI annotations @invariant/@decision/@covers/@writes/@synthesize — typed CAST metadata. (4) capability calls @io.print — the ONLY wrong one; caps take NO sigil (fixed in the guide, 163 call sites). CONSEQUENCE: any generic 'AtIdent in expression position' parser rule would silently swallow compiler directives and AI annotations as capability calls = silent miscompile. Any source-rewriting tool must NOT treat @ as one construct — note crush-notebook's @wip.started_by carries a DOT, so a 'strip @ from @x.y' rule eats it.  _(kai, 2026-07-14)_
- [ ] **issue** — ARCHIVE ARCHAEOLOGY (exosphere-1.0.zip 2026-02-24 + crush.zip 2025-12): POLYGLOT HAS NEVER WORKED IN ANY GENERATION. (1) crush.zip = the original Dec-2025 PYTHON prototype (casm.py/casavm.py) — no polyglot. (2) exosphere-1.0 archive/archived-stdlib/ = 137 caps across 23 namespaces incl polyglot.call/lib/transfer + python.stdlib/js.stdlib — but polyglot_bridge.rs (664 lines) is a MOCK: execute_polyglot_function() says 'For now, return mock results' and hardcodes (python,math,sqrt)->sqrt computed IN RUST, (python,json,loads)->fixed string. ZERO Command::new/pyo3/rustpython in the crate. It cannot execute Python at all. (3) in-tree crush-lang: variables:Vec::new() hardcoded (cece). (4) crush-ast today: same. DANGER: the mock returns PLAUSIBLE values (a real sqrt) so restoring it would PASS tests — a perfect silent-wrong-answer generator. Do not restore; build fresh with the rustpython AST route. ALSO: @io.print appears in ZERO archive files; io.print/io.echo/io.input existed and were ALWAYS unprefixed — confirms the guide fix. ALSO: the guide's sys./array./map./console./convert./type. namespaces existed in NO generation (archive used system./data./text./dom./binary./result.) — never real anywhere. ALSO: the archive's 137 caps vs today's ~26 = ~111 capabilities lost when the stdlib was archived; that is the real 'stdcap' divergence the captain remembered.  _(unknown, 2026-07-14)_
- [ ] **opportunity** — STDLIB RESTORATION MAP (from exosphere-1.0.zip archive/archived-stdlib, 137 caps / 23 namespaces vs today's ~26). Split by mock-density audit: CLEAN + RESTORABLE = 103 caps across text(555 LOC) fs(463) missing_capabilities(521) storage(263) bytes(222) result(161) binary(153) math(127) gfx(123) task(81) async_cap(62) — zero mock markers, real impls. MOCK-TAINTED = 46 caps across polyglot_bridge(664 LOC, 9 markers, execute_polyglot_function returns HARDCODED values), ai_capabilities(1481, 9), dom(780, 5) — DO NOT restore without rewriting; they return plausible fakes that pass tests. Partial: ics(490,2) buffer(249,1). USAGE SPEC: crush-capsules/demos/ (4 Rust demos) are the API-in-use reference — ai-data-analysis imports crush_stdlib::{ai_capabilities,polyglot_bridge} + nanovm, so ITS results were mock; advanced-capsules + distributed-orchestrator use exo_core capsule_engine/ecap (real infra); data-encryption-demo is just exo_core::crypto. All 4 demos are DEAD: path deps point at /workspace/core/{exo-core,crush-common} which do not exist.  _(unknown, 2026-07-14)_
- [ ] **issue** — REAL extraction regression (NOT @io.print/EXO-47 — those were false leads). Ran exosphere's OWN tests/language/*.crush corpus against crush-ast crush-run: 4/7 parse, 3 FAIL. (a) for_loop_test.crush: 'for i in 0..3 { }' range-based for loops DO NOT PARSE (Expected LBrace). (b) async_test.crush: 'Unexpected token in expression: Async'. (c) concurrency_structs.crush: assign-in-expression line 6. These PASS in exosphere/crates/core/crush-lang. crush-ast lost real language features in the extraction. exosphere's corpus IS the acceptance test for the fix. ALSO: crush-capsules is NOT an example source — tools/core + standalone/app-runtime + examples have ZERO .crush files (all Rust host-side capsule demos); only 2 .crush exist repo-wide: squad-bridge-peek/main.crush (7 LOC, parses) and games/snake/game.crush (559 LOC, 226 parse errors — written in a far richer Rust-like dialect with mut/generics/::/match =>/type annotations/-> returns). Snake is a ROADMAP SPEC for where the language was headed, not a borrowable example.  _(kai, 2026-07-14)_
- [ ] **issue** — Crush's + operator does not support string + number concatenation at all — 'io.print("x: " + 5)' fails with 'type error: expected numeric, got str'. This breaks crush-website/example.crush's own 'Python says 5^3 is: ' + result pattern once @io.print and @python marshaling are both fixed (found while verifying CRUSHAST-POLYGLOT-1 end-to-end against the real file). Out of scope for CRUSHAST-POLYGLOT-1 — logged for whoever owns operator semantics next.  _(unknown, 2026-07-14)_
- [ ] **issue** — SEVERE PRE-EXISTING: ANY struct declaration silently kills main. 'struct P { x } fn main() { print("hi") }' exits 0, steps=2, prints NOTHING. No error, no warning, zero exit code. Verified against the parent commit — not caused by the s385 parser work. main is never called. Struct programs have NEVER worked. concurrency_structs.crush now PARSES (7/7 corpus) but still cannot RUN because of this. This is the purest silent failure in the codebase and the flagship target for the khukuri-bugarium parking lot (workspace-meta/plans/2026-07-14-PARKING-LOT-bugarium-vs-crush.md) — the 'steps=' counter is a free oracle: assert main's body executed >=1 instruction.  _(unknown, 2026-07-14)_
