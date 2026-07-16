# Python Lowering Coverage: What's Cheap, What's Not

> Status: research / pre-design. Written 2026-07-14 during CRUSHAST-POLYGLOT-1
> (cece), answering the open question left by `crushvm-rustpython.md`:
> *"Exactly which Python feature set is CAST-lowerable vs. must fall back?"*
> Not yet acted on — no code changes here, just the analysis and a priority
> ranking for whoever picks it up.

---

## 1. The question that prompted this

CRUSHAST-POLYGLOT-1 shipped `@python { ... }` polyglot blocks that execute
via `EXEC_LANG` — a subprocess shell-out to real `python3`, with variables
marshaled across the process boundary via JSON (see
`crates/crush-lang-sdk/src/compile.rs::rewrite_python_marshaling` and
`crates/crush-vm/src/scheduler.rs::CRUSH_RESULT_SENTINEL`). Separately,
`crush-lang-python` already has a **transpile lane** — a hand-written
lowerer (`lower_stmt.rs`/`lower_expr.rs`) that turns Python source into CAST
via `rustpython-parser`, for programs simple enough to run natively in
CrushVM with no subprocess at all.

The question raised mid-session: given the team already rejected embedding
`rustpython-vm` as a second runtime (see §2), would porting *specific
features* from rustpython-vm into crush-vm's own native execution improve
things — i.e., shrink how often a polyglot block has to fall back to the
subprocess lane?

## 2. The settled part: no embedded second VM

This was already decided, independently, by two related projects, and this
doc does not reopen it:

- `docs/design/crushvm-rustpython.md` (this repo): *"RustPython's
  parser/AST is the high-value piece; its VM is optional... low/avoid."*
  Reasoning: two garbage collectors, two schedulers, two object models, two
  capability systems, two VMs fighting for authority.
- `exosphere/.jagent/planning/research/polyglot-runtime-consolidation.md`:
  exosphere *lived* with four embedded VMs (`boa_engine` ~137K LOC,
  `rustpython-vm`+PyO3 ~200K LOC, `mlua` ~50K LOC, plus NanoVM itself) and
  it was an active liability — `icu_*` version conflicts blocking builds,
  four GCs, four object models needing bridging. Their recommendation is to
  *tear out* embedded VMs in favor of exactly the pattern crush-ast already
  uses: every language lowers to CAST, one VM executes it.

So "port rustpython-vm's internal machinery" (its frame/object model, its
class engine, its bytecode loop) into crush-vm would be re-embedding a
second object model at finer grain — the same mistake, smaller pieces.
**Not recommended**, and not what the rest of this doc is about.

## 3. The open part: use rustpython-vm as a *reference*, not a dependency

The live opportunity is narrower: for Python constructs `crush-lang-python`
currently rejects, does Crush's own CAST already have a matching primitive
(pure lowering-code gap, zero VM work) — and for the ones that don't, is
rustpython-vm's *design* (not code) a useful blueprint for a Crush-native
primitive?

### 3a. What Crush's CAST already has (checked directly, `crush-cast/src/lib.rs`)

| Python construct | `crush-lang-python` today | Matching CAST primitive |
|---|---|---|
| `try`/`except` | `bail!("try/except not yet supported")` | `Statement::TryCatch` + `Throw` — **already exist and are wired end to end** (`ENTER_TRY`/`EXIT_TRY`/`THROW` opcodes, `try_stack` in `scheduler.rs`) |
| `match` | `bail!("match statements not yet supported")` | `Expression::Match` with `MatchArm`/`Pattern` — **already exists** |
| basic classes (data only) | `bail!("class definitions not yet supported")` | `Statement::StructDef` + `Expression::NewStruct` — **already exist, but fields-only: no methods, no inheritance, no MRO** |
| comprehensions | `bail!("comprehensions not yet supported")` | none needed — desugars to an ordinary loop + array/map, see §3b |

For the first three rows, the gap is **entirely in the lowering code**
(`lower_stmt.rs`/`lower_expr.rs`), not in crush-vm. No new opcodes, no
rustpython-vm code to port — just finish mapping Python AST shapes onto CAST
nodes that already compile and run today.

### 3b. rustpython-vm survey — what the genuinely-missing pieces would cost

Full survey results below; the short version: **generators and exceptions
are cheap to build natively, classes are expensive.**

**Generators (`yield`).** Crush's own `Expression::Yield` is a bare
cooperative hint (`thread.yielded = true` in `scheduler.rs`) — nothing like
Python's suspend-with-value-and-resume. rustpython-vm's approach turns out
to be a good blueprint: it does **not** use fibers or a compile-time
state-machine transform. It reifies the whole interpreter frame as plain
data — operand stack, block stack, and program counter all stored as heap
fields (`FrameState` in `frame.rs`) rather than living implicitly on Rust's
native call stack. "Resuming" is just re-entering the bytecode loop from a
saved program counter with that frame's saved stacks — no stack-switching.
~900 LOC total (`coroutine.rs` + `builtins/generator.rs` +
`builtins/coroutine.rs` + `builtins/asyncgenerator.rs`), plus reliance on
the interpreter loop's own resumability. This maps onto Crush's existing
green-thread model (`GreenThread` already keeps persistent per-thread
stack/call_stack/ip across scheduler rounds in `scheduler.rs`) better than
expected: a Crush generator would look like "a green thread with
heap-addressable frame state, one `YIELD_VALUE` op that suspends with state
intact, one resume entry point" — an extension of what's already there, not
new invention.

**Exceptions.** Confirms crush-vm's existing design is the right shape:
`Result`-based unwind through the interpreter loop via an explicit block
stack (`Block` enum: `Loop`/`TryExcept`/`Finally`/etc. in `frame.rs`), with
`except SomeType:` arms compiled as ordinary type-check-and-branch bytecode
— not special VM machinery. Crush already has exactly this shape
(`try_stack`, `ENTER_TRY`/`EXIT_TRY`/`THROW`). Zero new VM work; this is
purely a `lower_stmt.rs` job (map Python's `Try`/`ExceptHandler` AST nodes
onto `Statement::TryCatch`, with exception-type checks compiled as ordinary
`isinstance`-style branches at the handler).

**Comprehensions.** Fully confirmed free. No comprehension-specific
control flow exists in rustpython-vm's interpreter at all — they desugar
(in the separate compiler crate) to an ordinary `ForIter` loop plus one of
three tiny fast-path ops (`ListAppend`/`SetAdd`/`MapAdd`) that mutate a
collection sitting at a fixed operand-stack offset. Pure lowering-step
work for Crush: desugar in `lower_stmt.rs`/`lower_expr.rs` to a `For` loop
+ `arr_push`-equivalent, no new opcode strictly required (maybe one narrow
"insert at stack offset N" op if the existing primitives don't cover it
cleanly). Generator *expressions* are the one exception — those desugar to
an actual generator function, so they inherit whatever generators cost.

**Classes.** The expensive one. Real Python class semantics — C3 MRO
linearization (`PyType::resolve_mro` / `linearise_mro`) plus the
descriptor protocol (`__get__`/`__set__`, driving properties, bound
methods, `classmethod`/`staticmethod`, `__slots__`) — is 3200+ LOC in
rustpython-vm (`type.rs` 1508 + `slot.rs` 1332 + `descriptor.rs` 368),
deeply coupled to its own `PyObject`/`PyRef` object model, not extractable
as a library at any granularity. A "just enough" subset — single
inheritance, a plain instance dict, ordinary method dispatch, no `super()`,
no properties, no classmethods — is cheap (a few hundred LOC on top of
`StructDef`+`NewStruct`). But "enough to matter for real Python code" pulls
in MRO + descriptors, which is a multi-thousand-LOC undertaking even as a
from-scratch native reimplementation.

## 4. Recommendation / priority ranking

1. **Finish the free lowering wins first**: `try`/`except` → `TryCatch`,
   `match` → `Match`, comprehensions → desugared loops. Zero new VM
   primitives, all target CAST nodes that already compile and run. This
   alone should cover a large share of real-world polyglot blocks (most
   scripts that fail today aren't failing on classes or generators).
2. **Simple structs-as-classes** (fields + plain methods, no inheritance)
   is a reasonable next step if user demand shows up — small, bounded scope,
   built on `StructDef`/`NewStruct` as-is.
3. **Generators are worth a real design pass**, not because they're free,
   but because the reified-frame technique maps unexpectedly well onto
   Crush's existing green-thread scheduler. Scope as its own ticket if
   pursued — it's a new CASM primitive (`YIELD_VALUE` + resumable frame
   state), not a lowering-only change.
4. **Leave full OOP (MRO, descriptors, `super()`, decorators) on the
   subprocess fallback path.** The cost-to-value ratio is bad relative to
   just shelling out to real `python3` for programs that need it, and nothing
   here suggests that calculus changes.

## 5. What this doc is *not*

This doesn't cover the "real Python program with third-party packages"
case at all (`numpy`, `pandas`, arbitrary PyPI) — that's structurally a
subprocess-lane problem regardless of how much of core-language Python gets
natively lowered, since the whole point of reaching for a package is
running code Crush has no way to reimplement. The router concept from
`crushvm-rustpython.md` §2 (static-analysis-driven lane selection: CAST vs.
subprocess) is still the right frame for deciding, per-block, whether
native lowering is even attempted.

## References

- `docs/design/crushvm-rustpython.md` — the original three-lane design
  series this doc extends; see its §8 Open Questions.
- `exosphere/.jagent/planning/research/polyglot-runtime-consolidation.md`
  — independent "one VM" conclusion from the sibling project's lived
  experience with embedded VMs.
- `crates/crush-lang-python/src/{lower_stmt,lower_expr,analyzer}.rs` —
  current lowering coverage and its `bail!` list.
- `crates/crush-cast/src/lib.rs` — `Statement`/`Expression` enums referenced
  in §3a.
- `crates/crush-vm/src/scheduler.rs` — `GreenThread`, `SPAWN`/`AWAIT`/`YIELD`
  opcodes referenced in the generators discussion.
