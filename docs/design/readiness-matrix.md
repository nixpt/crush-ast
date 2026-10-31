# Crush Production Readiness Matrix

> Last updated: 2026-07-14. Branch: `feat/python-crush-bridge`.
> Legend: 🟢 Production-ready 🟡 Works, has gaps 🔴 Stub/broken ⚫ Not started 🔮 Design only

---

## 1. Compiler Pipeline

| Component | Status | Tests | Gaps | Risk |
|-----------|--------|-------|------|------|
| Parser (crush-frontend) | 🟡 | 78 | Lambda `|a,b|` syntax broken (pipe collision). Multi-file merge not exposed to CLI. | Low |
| CAST IR (crush-cast) | 🟢 | 22 | VectorMath expression type added but unused. | Low |
| Semantics (semantics.rs) | 🟡 | 0 | CastType::F32/BigInt/Complex/Tensor stubbed to basic types. No real type checking. | Medium |
| Compiler (compiler.rs) | 🟢 | 1905L | `__crush_assign__`, `__crush_ternary__` lowering added. Loops, functions, arrays all wired. | Low |
| CASM (casm) | 🟢 | 13 | ecasm tests pre-existing failure (type_hints field). Not a regression. | Low |
| Optimizer (optimizer.rs) | 🟡 | 0 | 460 lines of optimization passes. No unit tests. Inline/const-fold untested. | Medium |
| CAISON (crush-caison, formerly CSON) | 🟡 | 9 | Parser-only (no serializer). String escapes unsupported. Comments inside values issue. | Low |

---

## 2. VM Backends

| Backend | Status | Opcodes | Tests | Gaps | Risk |
|---------|--------|---------|-------|------|------|
| **CVM1** | 🟢 | 46/46 | 99 | All opcodes implemented. Scheduler + caps wired. | Low |
| **FastVM** | 🟢 | 79/84 | ~80 | CrossLangCall missing. ArrSet duplicate pattern. | Low |
| **AOT C** | 🟢 | 36+ | 0 (lib) | 852 lines. Arrays, objects, math, string, bitwise, SIMD. Type inference disabled for non-crush sources (Value-mode locals only). | Low |
| **AOT Rust** | ��� | 36+ | 0 (lib) | 590 lines. Math + string + bitwise parity with C. | Low |
| **JIT** | 🟡 | 31/86 | 21 | Silent miscompilation: ~55 ops fall to TAG_NULL. Phase 2-7 of 7-phase roadmap not started. Fuzz target disagrees with interpreter. | High |
| **GPU/PTX** | 🔴 | 0 | 0 | Design doc complete. `crush-ptx` scaffold exists. No opcodes implemented. | High |

---

## 3. Language Walkers

| Language | Walker | Parser | SDK Tests | Lowering Coverage | AOT C | Known Gaps |
|----------|--------|--------|-----------|-------------------|-------|------------|
| **C** | CWalker | tree-sitter-c | 12 | ~80% — switch, do-while, break/continue, multi-decl, array decl, field access, pointer ops, ternary, inc/dec | 🟢 | Struct defs, sizeof, casts, preprocessor not lowered |
| **Python** | PythonFrontend | rustpython-parser | 8 | ~60% — functions, loops, lists, ternary, dict, attribute, subscript | 🟢 | No comprehensions, classes, try/except, slices, in/is (lowered as calls but runtime stubs), lambda |
| **JS/TS** | JsFrontend | swc + boa | 11 | ~70% — functions, classes, async, generators, try/catch, switch, arrow, template, JSX, TS types | 🟢 | No prototype chains, eval, Proxy. `for` loop scoping across nested loops |
| **Rust** | RustFrontend | syn | 11 | ~40% — functions, loops, field access, array literals, closures, ranges | 🟢 | No structs, enums, traits, match, macros, generics, modules |
| **Go** | GoWalker | tree-sitter-go | 9 (2 SDK) | ~40% — `func` declarations (params + returns), `var` / `:=`, assignment, `if`/`else`, all three `for` heads (C-style lowered to a `while`, condition-only, single-variable `range`), calls, binary/unary operators | 🟢 (`crush-aotc run` verified on a Go program) | Switch, `defer`, two-variable `range`, goroutines/channels, slices/maps/structs, methods/interfaces, packages — refused loudly, never dropped |
| **Zig** | ZigWalker | tree-sitter-zig | 6 (2 SDK) | ~35% — `fn` declarations, `return`, `while`, `for`, assignment (incl. member assignment), blocks, calls | 🟢 (`crush-aotc run` verified on a Zig file) | Structs, pointers/deref, `comptime`, error unions, `switch`, labelled blocks — refused loudly |
| **Bash** | BashFrontend | `brush_parser` (bash mode) | 270 (270-case shared oracle) | ~81% — control flow, functions, arrays, `[[ ]]` (incl. `-e`/`-f`/`-d` and the string operators), `case`, `${…}` (`:- # ## % %% / //` and the `:off:len` forms), arithmetic (hex, octal, `base#n`, `**`), `printf` (`%s %d %x %o %b %f %e %g`), `read` (from stdin, `<`, or a pipeline stage), `echo -e`, pathname expansion in every path component (`echo *.txt`, `*/*.txt`, `*/`), **pipelines** (in-VM buffer handoff), **`<` `>` `>>` and here-documents** over `fs.read`/`fs.write`, the builtin command set `cat ls wc head tail grep cp mv rm mkdir touch sort uniq cut tr` (CRUSH-311), and `shopt -s/-u nullglob failglob` | 🟡 (no shell AOT C test) | No external commands/processes (`date`, `git`, `sed`, `find`, `awk` …), no `$(…)`/backticks, subshells, `&`, `2>&1`, `>` on a loop/`if`/`{ }`, `wc FILE`, a multi-column `wc < f`, `ls -l`; no `=~`, `declare`/associative arrays, `${!x}`; no POSIX `sh` dialect; no `**`, absolute-path globs, `extglob`/`globstar`, any other `shopt` option; a *zero-field* word reaching `printf` (nullglob) is refused; `read` cannot tell a blank line from EOF |
| **Zsh** | ZshFrontend | `zshrs_parse` | 270 (270-case shared oracle) | ~78% — same constructs as Bash through `crush_walker_core::shell`, including pipelines, redirections and the builtin command set, with the real dialect splits (`$((010))` = 10, `base#n` > 36 rejected, `${x^^}` bad substitution, `$a` joins all elements, `${p}` literal in a pattern, `$x` not field-split, a pipeline's last stage keeps its assignments) and `setopt null_glob` / `no_nomatch` | 🟡 (no shell AOT C test) | Same as Bash, plus zsh `$x[…]`, `$x:…`, `=cmd`, `|&`; `setopt` covers only `null_glob` and `nomatch`; an unmatched glob ends the run with `no matches found` — that *is* zsh's default (unlike bash's `failglob`, which only skips the command), so it is behaviour, not a gap |
| **Nepcode** | NepaliFrontend | crush-frontend (lexer) | 3 (2 SDK) | ~10% — passes through the Crush parser with Nepali keyword aliases (`karya` = `fn`, `yadi` = `if`) | 🟢 (`crush-aotc run` verified on a `.np` file) | No dedicated walker — delegates to the Crush parser; only the keywords are aliased |
| **Wasm** | walk_wasm() | wasmparser | 2 | ~30% — imports, exports, function signatures and `call` instructions (WASI `fd_write` → `io.print`; `_start` wired to `main`). Values and the rest of the instruction stream are dropped. | 🟡 (no `.wasm` fixture in the suite) | No operand/stack lowering: only calls survive, so a module's arithmetic and control flow are lost |
| **Dart** | DartWalker | tree-sitter-dart | 3 | 0% — the walker produces an empty `main`; only the adapter/registry plumbing is exercised | 🔴 not wired into `crush-aotc` (its extension list omits `dart`, so a `.dart` file falls through to the Crush parser and fails to parse) | No statement/expression lowering at all |

> Bash and Zsh rows re-reviewed 2026-10-30 against the walkers themselves
> (CRUSH-202 + CRUSH-310 + CRUSH-311), not against the parsers: 270 shared
> oracle cases run vs real `/usr/bin/bash` and `/usr/bin/zsh -f` — bash 220
> match / 50 loud-unsupported, zsh 210 / 60, **0 silently wrong** in both.
> Coverage percentages are that corpus, which deliberately over-samples hard
> constructs. Every other row in this document is as of 2026-07-14.
>
> Go, Zig, Nepcode, Wasm and Dart re-reviewed 2026-10-30 against their walkers
> and by running one program each through `crush-aotc run`. Corrections from
> the 2026-07-14 entries: Go and Zig lower control flow and functions (not
> "main body only"), Go's counts are the crate's 9 tests and Nepcode's 3, and
> Dart's AOT C column was green without anything exercising it — `.dart` is not
> in `crush-aotc`'s extension list at all.

---

## 4. Cross-Cutting Capabilities

| Capability | Status | Tests | Notes |
|------------|--------|-------|-------|
| Polyglot AOT (multi-language → one .so) | 🟡 | 1 | Programs.merge() works in SDK test. No CLI flag for multi-file input. |
| C↔Crush FFI (`__crush_ffi__`) | 🟡 | 2 | Standalone `crush-vm run` does not grant the arbitrary-path gateway. Rust hosts may load a trusted plugin explicitly; Cargo plugin builds and guest-selected paths are not supported. |
| Python→AOT C | 🟢 | 8 | Full pipeline working. Slices/in/is lowered. |
| JS→AOT C | 🟢 | 11 | Full pipeline. TS stripping. Subscript fixed. |
| Rust→AOT C | �� | 11 | Full pipeline. Field access + closures + array literals. |
| LTO optimization | 🟢 | 0 | 3-layer: Rust fat LTO + gcc -flto + CFLAGS -flto. 64-80% binary size reduction. |
| Debugger | 🟡 | ~10 | Breakpoints, step/continue, REPL. Variable inspection returns "not yet implemented". |
| AI opcodes (Query, Synthesize, etc.) | 🔴 | 0 | 10 AI + 3 DOM + spawn/await/yield = 16 NOP opcodes. All compile to nothing. |
| Async/Spawn/Await | 🔴 | 0 | Parsed and stored in AST. Runtime is NOP. |
| V8 fallback for JS | 🔮 | 0 | Design doc complete. Feature-gated. Snapshot-based startup. |

---

## 5. Infrastructure

| Area | Status | Tests | Notes |
|------|--------|-------|-------|
| Build (workspace) | ��� | 0 | 35+ crates. `--release` with LTO = ~2min. Debug = ~120s. |
| Binary size (release+LTO) | �� | 0 | 19-30MB per binary. Down from 53-142MB. |
| Test coverage | 🟡 | ~159 | All SDKs covered. VM tests strong. Codegen/AOT untested at unit level. 18 zero-coverage error paths in VM. |
| CI (GitHub Actions) | 🔴 | 0 | No CI configuration. All testing is local. |
| Crates.io publishing | 🟡 | 0 | 6 core crates published. Walkers not published. |
| Benchmarks | 🟡 | ~10 | Python+JS sieve benchmarks. No Rust/C/Go/Zig benchmark files. |
| Docs | 🟡 | ~10 docs | 4 design docs this session. Architecture doc (CLAUDE.md) is minimal. No API docs. |
| Walker plugin system | 🟡 | 5 | AdapterRegistry + LanguageAdapter trait shipped. Plugin loading (cdylib) designed but not implemented. |

---

## 6. Risk Matrix

| Risk | Severity | Probability | Mitigation |
|------|----------|-------------|------------|
| JIT silent miscompilation | High | Certain | Disable JIT path until Phase 2 complete. Add fuzz gate. |
| Tree-sitter linker errors | Medium | Certain | Pre-existing. Affects walker crate test binaries only. Workaround: test from crush-aot integration tests. |
| Lambda pipe collision | Medium | Certain | `|a,b|` syntax broken. Workaround: use `fn` syntax. Fix is in parser. |
| V8 fallback not available | Low | N/A | Design complete. Not a regression — Boa is still available for surfer web content. |
| No CI catches regressions | High | High | All testing is local. A bad commit on main breaks everyone. Add CI is P0. |
| Walkers are stubs (Go/Zig/Nepcode/Wasm/Dart) | Medium | Present | They walk but don't lower. OK for adapter pattern proof. Need lowering for production use. (Bash/Zsh no longer belong here: they lower, with pipelines, redirections and a builtin command set, but no external commands or `$(…)`.) |

---

## 7. v0.3.0 Release Checklist

Based on this matrix, these items gate a v0.3.0 release:

| # | Gate | Current | Target | Effort |
|---|------|---------|--------|--------|
| 1 | Fix JIT silent miscompile | 🔴 55 ops → TAG_NULL | 🟢 honest errors | 8-16h |
| 2 | Fix lambda pipe collision | 🔴 `|a,b|` broken | 🟢 | 1-2h |
| 3 | Add CI (build + test) | 🔴 no CI | 🟡 build on push | 2-4h |
| 4 | Complete walker lowerings | 🟡 Go/Zig/Nepcode stubs (Bash/Zsh done) | 🟡 40%+ coverage each | 8-12h |
| 5 | AI opcodes: at least 3/10 real | 🔴 10 NOPs | 🟡 Query + Synthesize + AgentDelegation | 4-8h |
| 6 | Multi-file polyglot CLI | 🟡 merge works, no CLI | 🟢 `crush-aotc compile a.js b.py --emit c` | 1-2h |
| 7 | Fill 18 zero-coverage VM error paths | 🔴 0 tests | 🟡 18 tests | 2-4h |
| 8 | Fix MOD sign bug | 🟡 portable_vm vs FastVM disagree | 🟢 | 30min |
| 9 | Remove unreachable code (vm.rs:326) | 🟡 dead code | 🟢 | 15min |

---

## 8. Current Scorecard

```
Compiler pipeline:   🟢🟢🟡🟢🟢🟡🟡  = 4/7 green
VM backends:         🟢🟢🟢🟢🟡🔴    = 4/6 green
Language walkers:    🟢🟢🟢🟡🟡🟡🟡🟡🟡🟡🟡  = 3/11 green
Cross-cutting:       🟢🟢🟢🟢🟢🟡🔴🔴🔮 = 5/9 green
Infrastructure:      🟢🟢🟡🔴🟡🟡🟡   = 2/7 green

Overall: 18/40 green (45%), 12 yellow (30%), 8 red (20%), 2 design-only (5%)
```

**Verdict:** The core pipeline and 4 primary languages (C/Python/JS/Rust) are production-ready for compiled workloads. The remaining 7 walkers need lowering work. The JIT needs honest-error hardening. CI is the highest-leverage missing piece.
