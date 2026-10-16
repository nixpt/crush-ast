# CRUSH-270 — `@lang` blocks declare their outputs explicitly: `@python -> names { … }` (#230)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-270 |
| **Priority** | P1 |
| **Status** | Done |
| **Phase** | M6 |
| **Assignee** | opencode |
| **Dependencies** | issue #225's analyzer fix, first or in the same PR (same binding analysis; see Related) |
| **Estimated effort** | M |
| **Filed by** | vega — 2026-10-15, owner decision on issue #230 |

## Problem

Today a polyglot block's **last top-level plain assignment** is implicitly marshaled back into the Crush
scope (`crates/crush-lang-python/src/analyzer.rs:46-54` picks it; `crates/crush-lang-sdk/src/compile.rs:167-190`
records it as `meta["polyglot_output"]`; `crates/crush-frontend/src/compiler.rs:1111-1122` stores it). The
data flow is invisible in the source. Measured on main `a1b7fdf` with `crush-run run --polyglot`:

| program | result |
|---|---|
| issue #230: block 1 `secret = 42`, block 2 `result = secret + 1` | `43`. `secret` silently became a Crush variable and was injected into block 2 |
| `result = 5 * 8` then `tmp = result + 1`, then `io.print(result)` | compile error **"Undefined variable: result"**: only the last assignment is exported, so the intended output is lost |
| Crush `let count = 10`; block ends with `count = 0`; `io.print(count)` | **`0`**: the block **silently overwrote a Crush variable** |
| Crush `let n = 5`; a block with no detected inputs | `n` is **not** a Python name; it's only `os.environ["n"] == "5"` (the fallback at `crates/crush-frontend/src/compiler.rs:286-288`) |

Each block does run in its own `python3 -c` process (`crates/crush-vm/src/scheduler.rs:105-112`, `:309-371`), so
the documented "fresh interpreter" is literally true. The surprise is entirely in the implicit output rule.

## Decision (owner, 2026-10-15)

**Outputs are explicit.** Inputs stay automatic (names a block reads are injected, as today).

```crush
fn main() {
    let count = 10
    @python -> total {
        tmp = 3
        total = tmp * 2
    }
    io.print(total)   // 6
    io.print(count)   // 10, untouched
}

@python -> (mean, stdev) { ... }      // several outputs
```

- `-> name` or `-> (a, b, …)` on the block header lists the names marshaled back. Each listed name becomes
  (or assigns) a Crush variable *visibly at that line*; nothing else leaves the block.
- A listed name the block never binds is an error. Prefer compile time (the analyzer knows the block's
  top-level bindings); at run time, a missing output must be a clear error naming the variable, never `null`.
- Applies to every `@lang` with marshaling (python today; check javascript/bash and keep them consistent).
- **Transition, one release:** a block **without** `->` keeps today's implicit rule but gets a **compile warning**
  naming the variable it exports and suggesting `-> <name>`. In the release after, no header means no outputs.
  The warning must also fire for the clobber case (an implicit export that overwrites an existing Crush variable).
- **Fix the env-only fallback in the same change:** Crush variables reach the block by name (via the same prologue
  as analyzer-detected inputs) or not at all. No more "only in `os.environ`".

## Success criteria

- [ ] Parser and AST: `@lang -> name { … }` / `@lang -> (a, b) { … }`; `use @lang` splicing still works.
- [ ] Marshaling uses the declared outputs; a declared-but-unbound name is a loud error.
- [ ] Implicit outputs warn (compile time) with the variable name and the suggested `->`. The clobber case warns.
- [ ] The env-only fallback is gone: injected variables are Python names.
- [ ] Tests for each row of the table above, plus multi-output and `use @lang` + `->`.
- [ ] Docs: `crates/crush-lang-sdk/README.md:55-72` (marshaling rules), `examples/crush/polyglot/*`
      (migrate them to `->`), and the isolation note in `examples/crush/polyglot/crush_imports.crush`.
- [ ] Engines: lowering stays `exec_lang`, so every engine that runs `@lang` today behaves the same. The JIT and
      FastVM keep refusing or yielding exactly as now.

## Related

- Issue #230 (this decision), issue #225 (the analyzer never looks inside comprehensions, lambdas or `def`
  bodies, so reads there aren't injected). #225 is a separate bug in the same pass. Fix it first or in the same
  PR, because the explicit-output check relies on the same binding analysis.

## Non-goals

- A persistent interpreter or state shared between blocks (rejected in CRUSH-224/225; still rejected).
- Changing how inputs are detected (automatic, by free-variable analysis).

## Resolution (opencode, 2026-10-15)

Implemented on `agent/opencode/CRUSH-270-explicit-outputs`, based on
`agent/opencode/issue-225-comprehension-reads` (the #241 analyzer fix this
builds on).

**Wire protocol for multi-output.** `meta["polyglot_output"]` (a single
string) is replaced by `meta["polyglot_outputs"]` (a JSON array of names). The
exec_lang spec now carries `out_count` plus `out_0..out_{k-1}`, mirroring the
existing `var_count`/`var_i` input convention. The Python/JS epilogue prints a
single sentinel line whose payload is a JSON *object keyed by name*
(`{"a": 1, "b": 2}`) instead of one scalar. `parse_exec_lang_stdout` is
unchanged in shape; a new shared helper `crush_vm::scheduler::exec_lang_outputs`
extracts each declared name from the object and the scheduler + `PortableVm`
push the values in reverse (so the compiler's first `store` sees the first
declared value). The compiler emits one `store` per declared output, in
declaration order. FastVM/JIT are untouched — they keep yielding/refusing
exactly as before.

**Warning channel.** No general compiler-warning channel existed (only
`ai_runtime::Warning`). Added `crush_vm::Program::warnings: Vec<String>` (not
serialized into the `.cvm1` blob, same as `source_map`).
`prepare_polyglot_blocks` now returns `Result<Vec<String>>`;
`compile_crush_to_casm_with_warnings` carries them through to the VM program,
and `crush-run` prints each to stderr with a `warning:` prefix (`crushc` too).
Two warnings: the implicit export (`… exports \`x\` implicitly; add \`-> x\``)
and the clobber (`… overwrites the existing Crush variable \`x\` …`).

**`->` support by language.** python, javascript/js/node (the languages with a
wired free-variable analyzer) marshal declared outputs and accept `->`. bash and
every other `@lang` has no marshaling, so `->` on them is a loud compile error
(`\`->\` output lists are not supported for @bash: it has no value marshaling`).

**Declared-but-unbound vs conditionally-bound.** The analyzer now exposes
`bound_anywhere` (every module-scope binding, including conditional ones inside
`if`/`try`/…). A declared name not in `bound_anywhere` is a compile error naming
it. A name bound only conditionally compiles, but a run-time miss raises
`NameError` in the epilogue, which the block turns into a clear error naming the
variable — never `null`.

**Env-only fallback removed.** `compiler.rs` no longer passes every earlier
Crush local as env vars when a block has no detected inputs. Inputs reach the
block by name (the same prologue as analyzer-detected inputs) or not at all.

