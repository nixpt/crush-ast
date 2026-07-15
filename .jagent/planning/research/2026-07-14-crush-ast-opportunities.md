# crush-ast opportunities — what the language-implementation world does that we don't

**Ticket:** SQ-RESEARCH-BYOX · **Author:** panini · **Date:** 2026-07-14
**Method:** read [build-your-own-x](https://github.com/codecrafters-io/build-your-own-x)'s
"Build your own Programming Language" section (43 tutorials) and the primary sources it links
to; grounded every claim against `crush-ast` **at HEAD**, read-only, plus a throwaway probe
crate built against a clean `/tmp` clone.
**crush-ast was not modified.** No edits, no fixes, no "while I was in there".

> **Companion:** `research/language-implementation.md` (the research-drive entry) has the
> full technique catalogue with citations. This report is only the *delta* against crush-ast.

### Provenance — read this before acting on any line item

Every claim below is pinned to **crush-ast HEAD = `e1d5595`**, read via a clean `/tmp` clone.
I did **not** read the captain's working tree, which currently carries **~30 modified files +
~12 untracked** of uncommitted work in flight.

I checked each finding-site against that WIP:

| Finding site | State in the captain's tree |
|---|---|
| `crush-jit/src/compiler.rs` *(#1)* | ✅ clean — untouched |
| `crush-frontend/src/parser/lexer.rs`, `parser/mod.rs` *(#3, #4)* | ✅ clean — untouched |
| `crush-cast/src/lib.rs` *(#3)* | ✅ clean — untouched |
| `crush-vm/src/vm.rs`, `memory.rs` *(#7)* | ✅ clean — untouched |
| `crush-vm/src/fastvm/instructions.rs`, `portable_vm.rs` | ⚠️ **dirty — captain WIP** |

So **#1, #3, #4 and #7 stand as written.** The two dirty files feed the *opcode counts* quoted
in #1 and #6 (~86 `FastOp`s, ~83 `PortableVm` arms) — those exact numbers may have moved, but
the JIT's silent-`null` catch-all is in a file the captain has not touched, so the defect itself
is unaffected. **Re-confirm the counts against the captain's tree before writing the #1 test.**

Also note: `crates/casm/src/lib.rs` does not currently compile in the captain's working tree
(`CasmError::UnknownInstruction` is referenced but not defined). That is **their in-flight edit,
not a defect at HEAD** — HEAD builds — and it is *not* reported as a finding. It is only
mentioned so nobody re-discovers it and files it as a bug.

---

## The meta-finding (read this before the ranked list)

The individual bugs below are not independent. They are **five instances of one pattern**:

> **crush-ast repeatedly builds both ends of a feature and never connects the middle — and
> has no test that would notice.**

| # | Both ends exist | The middle is missing |
|---|---|---|
| 1 | `CsonParseCap` implemented (`vm_cap.rs`) | never registered → `cson.parse` doesn't exist at runtime *(known, s379)* |
| 2 | `SourceLocation{line,col}` on every token; `casm::debug_info::SourcePos` + `Program::with_source_map` | the parser drops location; the AST has none |
| 3 | `Expression::Lambda` in the AST, compiled at `compiler.rs:1588`, 5 lambda rules in the tree-sitter grammar | the lexer/parser **cannot produce one** |
| 4 | `crush-jit` compiles ~31 FastOps | the other ~55 silently become `null` |
| 5 | `memory.rs` has a real mark-sweep GC | the production VM uses a different value model and never calls it |

Every one of these would have been caught by the **single thing crush-ast lacks**: an
executable conformance corpus. `crates/tree-sitter-crush/test_lambda.crush` **already exists
as a file** documenting lambda syntax — it is simply never run through the real parser. That
reframes the ranking below: the test harness is not hygiene, it is the mechanism that stops
this pattern from recurring.

---

## Ranked opportunities

Ranked by **(severity × leverage) ÷ effort**. #1 is the only one shipping *wrong answers*
today.

---

### 1. Make `crush-jit` bail out instead of silently miscompiling  ⚠️ CORRECTNESS

**What.** Replace the JIT's catch-all match arm — which pushes `null` for any opcode it
doesn't implement — with an `Unsupported` error, so `JitCompiler` *refuses* the function and
the caller falls back to the interpreter.

**Evidence.** `crates/crush-jit/src/compiler.rs:421`:
```rust
_ => { let cv = iconst(b, TAG_NULL); push(b, ctx, cv); }
```
The JIT names ~31 `FastOp`s (`PushInt/Float/Bool/Null`, `Add`…`Ge`, `Jump/JumpIf/JumpIfNot`,
`Load/StoreLocal`, `Dup/Swap/Pop`, `Not/And/Or`, `Halt/Return`, `Nop`) out of **~86 variants**
defined in `crates/crush-vm/src/fastvm/instructions.rs`. So `PushStr`, `Call`, and every
array/map/string/capability/AI opcode **JIT-compile to a silent `null`**. The same program can
return a different, wrong answer under the JIT than under the interpreter, with no error
raised. Contrast `crates/crush-vm/src/fastvm/execution.rs`, which returns
`Err(FastError::TypeMismatch)` and fails *loudly*.

The universal practice is the opposite: **always keep the slow correct path, and let the fast
path decline.** Sandler's C compiler keeps `replace_pseudos.ml` (spill everything to stack
slots) as a permanent fallback *underneath* graph-colouring register allocation
([nqcc2/lib/backend](https://github.com/nlsandler/nqcc2/tree/main/lib/backend)); LLVM's
`mem2reg` promotes allocas *where it can* and silently leaves the rest in memory
([LangImpl07](https://llvm.org/docs/tutorial/MyFirstLanguageFrontend/LangImpl07.html)).
Neither ever fabricates a value. Cranelift — *our own JIT backend* — ships a
`cranelift-fuzzgen` target whose entire job is asserting that JIT-compiled code and the
Cranelift interpreter agree
([wasmtime/fuzz](https://github.com/bytecodealliance/wasmtime/tree/main/fuzz)).

**Where it lands.** `crates/crush-jit/src/compiler.rs` (the catch-all at :421);
`crates/crush-jit/src/lib.rs` (`JitEngine` — the caller that must fall back to `FastVm`).

**Why it matters for us.** This is the same silent-`null` bug class already in panini's scroll
(*"`PushNull` must yield `Some(RuntimeValue::Null)`, not `None`"*) — which means it is
**systemic, not incidental**. A JIT that returns a wrong answer instead of an error is worse
than no JIT: every downstream consumer (nimbus's capsule VM) trusts it.

**Effort.** **Hours.** The fix is to make the catch-all return an error and thread one
fallback branch through `JitEngine`.

**Ticket.** `crush-jit: replace the silent TAG_NULL catch-all (compiler.rs:421) with Unsupported → fall back to FastVm; add a test that every FastOp either compiles or is refused.`

---

### 2. A conformance corpus + one runner across all four engines

**What.** Expectation-annotated `.crush` files plus a black-box runner that asserts **stdout,
stderr, and exit code** — run against *each* execution path.

**Evidence.** Crafting Interpreters and mal converged on this shape independently.
- Nystrom's suite ([munificent/craftinginterpreters/test](https://github.com/munificent/craftinginterpreters/tree/master/test),
  runner `tool/bin/test.dart`): tests are **programs in the target language**, expectations are
  **comments** — `// expect: 579`, `// expect runtime error: <msg>`, `// [line N] Error ...`.
  Exit codes are part of the contract (`65` = compile error, `70` = runtime error). Because it
  never touches an internal API, **the same suite validates both jlox and clox**. He wrote it
  *before either interpreter existed* and it *"found countless bugs."*
- mal ([kanaka/mal](https://github.com/kanaka/mal), `impls/tests/` + `runtest.py`, 386 lines,
  no framework): one suite drives **80+ implementations across ~98 languages** by spawning each
  as an opaque subprocess. Key trick: **values matched exactly, diagnostics matched by regex**
  (`;=>` vs `;/`) — that's how one corpus stays green across implementations whose error wording
  differs. Incremental levels are a directory ladder (`step0`…`stepA`) or, in clox, a
  path→`pass`|`skip` map.

**Where it lands.** New top-level `tests/conformance/` in `crush-ast` + a runner (an `xtask`
subcommand — `xtask/src/main.rs` is currently `fn main() {}`, i.e. an empty slot waiting for
exactly this). The corpus seed already exists: **80 `.crush` files** across `examples/crush/`
and `crates/tree-sitter-crush/`, none of which currently carry expected output (`grep` for
`expect:` across `examples/` and `crates/*/tests` = **0 hits**).

**Why it matters for us.** We have **four** execution engines — `PortableVm` (production),
`FastVm`, `crush-jit`, `crush-aot` — with **divergent value models and divergent opcode sets**,
and *nothing* asserts they agree on the same program. This one harness is the mechanism that
would have caught opportunities #1, #3, #4 and the known crush-cson bugs. It is the highest-
leverage single investment available.

**Effort.** **Days** for the runner (2–3), then ongoing for the corpus. Start with the 80 files
that already exist.

**Ticket.** `crush-ast: add tests/conformance/ (expect-annotated .crush) + an xtask runner asserting stdout/stderr/exit-code across PortableVm, FastVm, crush-jit, and crush-aot.`

---

### 3. Wire source locations through the AST

**What.** Give `CAST` nodes a typed `Span`/`SourcePos`, populate it in the parser, and thread
it to the bytecode source map — so diagnostics can point at source and runtime errors can
produce a stack trace.

**Evidence — this is a wire that is already soldered at both ends and cut in the middle:**
- **The producer exists:** `crates/crush-frontend/src/parser/lexer.rs:41` defines
  `pub struct SourceLocation { line, col }` (`Copy`, `Default`) and **every token carries one**.
- **The consumer exists:** `crates/casm/src/debug_info.rs` defines `SourcePos { line, col, file }`
  *and* span types (`start_line`/`end_line`); `crates/crush-vm/src/bytecode.rs:180` has
  `source_map: Vec<(usize, usize)>` and `Program::with_source_map`.
- **The middle is cut:** the AST has **zero** location fields — `grep -cE 'line|col|span|offset'`
  on `crates/crush-cast/src/lib.rs` = **0**. Nodes carry only an untyped
  `meta: HashMap<String, serde_json::Value>`, and the parser constructs **every** node with
  `meta: HashMap::new()` (`grep -c 'insert("line"'` on `parser/mod.rs` = **0**).
  `compiler.rs:1945` (`create_instr`) reads only `meta["lang"]`, never line/col.
- **Consequence, verified:** `crush-vm`'s `VmError` is location-free — a runtime failure reports
  `"stack underflow"` with **no line, no function, no call stack** (`grep` for
  `backtrace|stack_trace` in `crates/crush-vm/src` = 0 hits). And the only source map that
  *does* get built (`assembler.rs:147`) maps **CASM assembly line numbers**, not `.crush` source
  lines.

**Reference designs.** Crafting Interpreters
[chunks-of-bytecode §Line Information](https://craftinginterpreters.com/chunks-of-bytecode.html)
(and its RLE challenge — note `getLine()` is only called *on the error path*, so it is allowed
to be O(n); **never let debug info cost the hot path**), and
[calls-and-functions §Runtime error reporting](https://craftinginterpreters.com/calls-and-functions.html)
for the stack-trace walk. Racket's syntax objects are the architectural argument: source
location is a **first-class part of the IR contract**, not an optional side-channel
([beautifulracket](https://beautifulracket.com/stacker/the-reader.html)) — which is exactly what
a pluggable multi-language front-end like ours needs.

**Where it lands.** `crates/crush-cast/src/lib.rs` (add the field),
`crates/crush-frontend/src/parser/mod.rs` (populate from the token's `SourceLocation`),
`crates/crush-frontend/src/compiler.rs:1945` (`create_instr` → `casm::debug_info`),
`crates/crush-vm/src/vm.rs` (`VmError` carries a position), `crates/crush-diagnostics`
(render a caret/underline — today `DiagRecord` has only `line`/`col` as `Option<u32>` and
there is no `ariadne`/`codespan`/`miette` dependency anywhere).

**Why it matters for us.** This is the single highest-leverage *language-quality* change
available to the front-end, and it is **much cheaper than it looks** because both ends already
exist. It also unblocks decent diagnostics, which Nystrom argues is where the time should go:
*"If your goal is just to implement a language and get it in front of users, almost all of
[parsing theory] doesn't matter"* — spend it on error messages instead.

**Effort.** **Days**, not a wave — it is connecting an existing wire, not a from-scratch span
project. (Add a `span` field + populate at ~60 parser construction sites + thread through
`create_instr`.)

**Ticket.** `crush-cast: add a typed Span to CAST nodes; populate from the lexer's SourceLocation in crush-frontend's parser; thread to casm debug_info so VmError can carry a .crush line + stack trace.`

---

### 4. Lambdas are unreachable from crush source — a "for now" lexer shortcut disabled a whole language feature

**What.** Fix the lexer/parser so `|x, y| { ... }` actually parses. Then decide, separately and
deliberately, whether crush gets **real closures**.

**Evidence — empirically verified** against a clean HEAD clone (probe crate, `/tmp`, read-only):
```
A: |a, b| { return a + b; }   → PARSE ERROR at the comma  (line 2, col 17)
B: |x, y| => x * y            → PARSE ERROR at the comma  (line 2, col 17)
C: |x| => x + n               → PARSE ERROR at FatArrow   (line 3, col 20)
D: fn add(a, b) { ... }       → parses + compiles fine     (control)
```
Root cause, `crates/crush-frontend/src/parser/lexer.rs:700-710`:
```rust
'|' => {
    self.advance();
    if self.peek() == Some('>')      { Ok(Token::Pipe(location)) }   // |>
    else if self.peek() == Some('|') { Ok(Token::Or(location)) }     // ||
    else { Ok(Token::Ident("|".to_string(), location)) }  // Single | as ident for now
}
```
A bare `|` lexes to **`Token::Ident("|")`** — an *identifier*. But the lambda parse path (the
sole `Expression::Lambda` construction site in `parser/mod.rs`) expects **`Token::Pipe`**, which
is `|>`. So the only lambda syntax the real parser accepts is `|> x |> => ...`, which collides
head-on with the pipe operator and which nobody writes.

Meanwhile `Expression::Lambda` **exists** in `crush-cast`, `compiler.rs:1588` **compiles** it
(lifting to a named top-level `__lambda_N`), the tree-sitter grammar has **5 lambda rules**, and
`crates/tree-sitter-crush/test_lambda.crush` **documents `|a, b| { ... }` as the syntax**. So
**our two front-ends disagree about the language**: tree-sitter accepts what the hand-written
parser rejects.

Note also the lexer's fallback turns **any** unrecognised operator char into an `Ident` rather
than a lex error (there's a matching *"Single & as ident for now"* at :697) — so typos silently
become identifiers instead of erroring.

**The closure half is a separate, bigger decision.** Even once `|x|` parses, lifting a lambda to
a named top-level function **is not implementing closures** — it only works when nothing is
captured. `crush_vm::vm::Value` has **no** `Function`/`Closure` variant (variants are
`Null/Bool/Int/Float/Str/Array/Map/Error/Bytes/thread-handle`), and the `CALL` opcode takes a
const-pool index naming a function **by name** (`crates/crush-vm/src/bytecode.rs` header). So
functions are not first-class values, and higher-order functions taking a capturing lambda are
not expressible. Crafting Interpreters
[ch. 25 "Closures"](https://craftinginterpreters.com/closures.html) is the reference design
(open vs closed upvalues; the open-upvalue list sorted by descending stack address so two
closures capturing the same local share one `ObjUpvalue`). Nystrom flags `resolveUpvalue()` as
the hardest function in the book — *"I found this function really challenging to get right the
first time"* — and ships a regression test for a real bug in it.

**Where it lands.** *(a)* `crates/crush-frontend/src/parser/lexer.rs:700-710` (emit a real
lambda-delimiter token) + `parser/mod.rs` (the `Expression::Lambda` path).
*(b)* `crates/crush-vm/src/vm.rs` (a closure `Value`), `bytecode.rs` (call-by-value), and
`compiler.rs` (upvalue resolution).

**Why it matters for us.** A documented language feature, with a grammar and compiler support,
that **cannot be written**. Whatever the closure decision, the current state — where
`test_lambda.crush` is a lie — should not persist.

**Effort.** *(a)* the lexer/parser fix: **hours**. *(b)* real closures: **a wave**, and a
language-semantics decision (captain's call), not a bug fix.

**Ticket.** `crush-frontend: lexer maps bare '|' to Ident (lexer.rs:700-710) so lambdas cannot parse; emit a lambda-delimiter token and make Expression::Lambda reachable. Separately: RFC whether crush gets real closures (Value has no closure variant; CALL is by-name).`

---

### 5. Fuzz the parser — we have zero fuzzing, and it is exactly why the cson bugs shipped

**What.** A `fuzz/` crate with `cargo-fuzz` targets asserting **properties**, not examples.

**Evidence.** crush-ast has **no fuzzing and no property testing of any kind**: no `fuzz/` dir,
no `cargo-fuzz` target, and no `proptest`/`quickcheck`/`arbitrary` in any crate's `Cargo.toml`.
The template to copy is Ruff's — the closest analogue (a Rust language front-end)
([astral-sh/ruff/fuzz](https://github.com/astral-sh/ruff/tree/main/fuzz)):
`ruff_parse_simple` (never panic; **and spans must never land mid-UTF-8-codepoint**),
`ruff_parse_idempotency` (parse∘print∘parse == parse), `ruff_formatter_idempotency`,
`ruff_fix_validity` (a fix must not introduce new errors).
The [rust-fuzz trophy case](https://github.com/rust-fuzz/trophy-case) says the #1 bug in a
hand-written recursive-descent parser is **stack overflow from unbounded recursion** —
confirmed in `sqlparser`, `toml`, `ron`, `pdf`. We have a hand-written recursive-descent parser.

**Where it lands.** New `fuzz/` crate at the `crush-ast` workspace root.
`crush-frontend`'s `Parser::parse(&str) -> Result<Program, Vec<ParseError>>` is a *ready-made*
fuzz target — "never panic on arbitrary input" is a one-property harness against an existing
signature. `crush-frontend`'s `render.rs` (1,499 lines — an AST printer) gives us the
round-trip property (`parse ∘ render ∘ parse == parse`) for free.

**Why it matters for us.** The known crush-cson bugs (silent `#`-comment corruption; comma-
truncated annotations; `\"` a hard parse error) are *precisely* the class a 20-line fuzz target
or a round-trip property finds in minutes. Adopt SQLite's `fuzzcheck` discipline while you're
there: **every crashing input a fuzzer ever produced gets minimized (`cargo fuzz tmin`) and
committed as a permanent regression test** — the fuzz corpus *becomes* the regression suite.

**Effort.** **Hours** for the first no-panic target; **days** for the full set + corpus.

**Ticket.** `crush-ast: add fuzz/ (cargo-fuzz) with parse_no_panic + parse_render_roundtrip targets over crush-frontend::Parser and render.rs; commit minimized crashes as regression tests.`

---

### 6. Differential-test the four engines against each other (zero-oracle)

**What.** Generate/collect programs, run them on `PortableVm` **and** `FastVm` **and**
`crush-jit`, assert identical results. **No expected outputs required.**

**Evidence.** This is the strongest correctness technique in the survey precisely because it
needs no oracle:
- **Rustlantis** (OOPSLA 2024, [ralfj.de](https://www.ralfj.de/blog/2024/11/25/rustlantis.html))
  generated random MIR and diffed optimisation levels and codegen backends against **Miri**, the
  reference interpreter → **22 new bugs**, 8 in rustc and 14 in backends (12 of those in LLVM,
  *which had already been heavily fuzzed*).
- **acwj's "triple test"** ([Part 60](https://github.com/DoctorWkt/acwj/tree/master/60_TripleTest)):
  build the compiler with itself twice; the two binaries **must be byte-identical**. No expected
  outputs, no test cases — and it caught codegen bugs that **149 hand-written regression tests
  missed** (a register-lifetime bug in ternaries; a 32-vs-64-bit `cmp` where `-1` sign-extended
  and compared as *positive*, so a loop never terminated).
- **Cranelift's own `cranelift-fuzzgen`**: compile a CLIF function to the host, *also* run it in
  the Cranelift interpreter, assert the results match.

**Where it lands.** Same `fuzz/` crate as #5 (`differential_eval.rs`), and/or the `xtask` runner
from #2. The oracle is free: `PortableVm` (`crates/crush-vm/src/portable_vm.rs`) *is* our
reference interpreter.

**Why it matters for us.** We have four engines with divergent value models
(`vm::Value` with `Rc<RefCell>` vs `value::RuntimeValue` + `Arena`) and divergent opcode sets
(~83 arms vs ~86 `FastOp`s vs ~31 JIT-compiled). Opportunity #1 is *exactly* the bug class this
finds automatically — and finds the next one too. Bisect stage-by-stage the way acwj does
(assert identical token counts, then diff dumped ASTs) to localise a divergence.

**Effort.** **Days** once #5's `fuzz/` scaffolding exists (it shares the harness).

**Ticket.** `crush-ast: add a differential fuzz target — same program through PortableVm / FastVm / crush-jit, assert identical result. PortableVm is the reference oracle.`

---

### 7. Decide the memory model: `PortableVm` leaks cycles, and the GC is on the *other* value model

**What.** Reconcile the two value/heap models, or explicitly document the limitation.

**Evidence.** crush-ast has **two divergent value+heap models in one crate, and only one has a GC**:
- `crush_vm::vm::Value` — used by **`PortableVm`, the production VM** — represents `Array` and
  `Map` as `Rc<RefCell<...>>` (`crates/crush-vm/src/vm.rs:78-80`). That is **pure reference
  counting: reference cycles leak and are unreclaimable**, and there is **no collector at all**.
- `crush_vm::value::RuntimeValue` + `memory.rs`'s `Arena` — used by `fastvm` and `crush-jit` —
  has a **real mark-sweep GC** (`Arena::mark`/`trace`/`sweep`), threshold-triggered from
  `FastVm::collect_garbage` (`fastvm/mod.rs:165`).

So the VM that actually ships has no GC, and the GC that exists is on the model that doesn't ship.

**Reference.** Crafting Interpreters'
[GC chapter](https://craftinginterpreters.com/garbage-collection.html). Two things to internalise
*before* touching this: **(a)** the hazards, not the algorithm, are the hard part — every one has
the shape *"an object is live only in a local while you allocate again"*; **(b)** the actual
deliverable is **`DEBUG_STRESS_GC`** (collect on *every* allocation), which turns a rare
heisenbug into a deterministic one. Nystrom: *"GC bugs are the worst bugs… you're looking for the
absence of code which fails to prevent a problem."* Build the stress flag before you need it.

**Where it lands.** `crates/crush-vm/src/vm.rs` (the `Value` enum),
`crates/crush-vm/src/portable_vm.rs`, `crates/crush-vm/src/memory.rs` (the existing `Arena`).

**Why it matters for us.** `let a = []; a.push(a)` leaks forever in the production VM today.
That may be an acceptable, *stated* limitation — plenty of languages ship refcounting and say so
out loud. What is not acceptable is that it's currently **unstated**, and that `memory.rs`'s
docs advertise *"Automatic Garbage Collection"* for a collector the shipping VM never calls.

**Effort.** **A wave** — and genuinely a design decision (captain's call), not a bug fix. The
cheap first step is a decision record + a doc fix, not code.

**Ticket.** `crush-vm: RFC the memory model — PortableVm's Rc<RefCell> Array/Map leaks cycles and has no collector, while memory.rs's mark-sweep Arena is only reachable from fastvm/JIT. Decide: unify, or document refcounting as a stated limitation (and fix memory.rs's misleading docs).`

---

## What crush-ast already does — and in several cases, better than the tutorials

This is a finding too, and it should change what we *don't* spend time on.

- **The parser is a real Pratt parser with proper precedence.** `parser/mod.rs:1099`
  (`parse_expression_with_precedence(min_prec)`), documented as such at the top of the file. This
  is Crafting Interpreters' [compiling-expressions](https://craftinginterpreters.com/compiling-expressions.html)
  chapter, already done. **Do not "add Pratt parsing" — we have it.**
- **Parser error recovery is better than most of the tutorials.** `Parser::parse` returns
  `Result<Program, Vec<ParseError>>` — **multi-error**, with a `synchronize()` routine and an
  iteration-limit guard. acwj dies on the first error; Sandler's blog compiler dies on the first
  error. We already collect and resynchronise. *(Worth checking the `canAssign` trap separately —
  Nystrom flags that a naive Pratt table silently accepts `a * b = c`.)*
- **The constant pool is already interned.** `crates/crush-vm/src/assembler.rs:55-64` has a real
  `intern()` with a `HashMap<String, usize>` dedup. That's CI's
  [string-interning](https://craftinginterpreters.com/hash-tables.html) section, done.
- **We already took acwj's Part 63 conclusion on day one.** acwj spends **62 parts** hand-rolling
  x86 codegen, then replaces it with a QBE IL emitter and finds the output *halves in size* —
  his stated regret is the hand-rolled backend. `crush-jit` uses **Cranelift** and `crush-aot`
  emits **C**. We skipped the 62 parts of pain. (This is why opportunity #1 is a *bail-out* fix,
  not a codegen project.)
- **Constant folding + propagation exists** (`crates/crush-frontend/src/optimizer.rs` — folds and
  propagates consts at the AST level, same placement as acwj
  [Part 44](https://github.com/DoctorWkt/acwj/tree/master/44_Fold_Optimisation)).
- **A latent asset worth naming:** acwj *cannot* add peephole optimisation because it `fprintf()`s
  assembly straight to a file — *"would require refactoring how assembly code is generated and
  stored."* We build **`casm::Instruction` values in memory**. So a peephole pass is available to
  us whenever we want it, at no architectural cost. We paid that price already; we should know we
  own the asset.

---

## Explicitly out of scope / not recommended

- **NaN boxing.** Tempting, and a model will suggest it. It's worth **~10%** and Nystrom says he
  *"might not [reach for it] first."* Our `RuntimeValue` is a tagged enum — fine. There is far
  more upside in #1–#3.
- **Hand-rolling SSA or a register allocator.** We use Cranelift; it does this for us. LLVM's own
  guidance is that a front-end reproducing SSA construction is *"inconvenient and wasteful."*
- **A `%`→`&` hash probing fix.** CI's biggest measured win (2×) doesn't apply: our tables are
  Rust's `std::collections::HashMap`, not a hand-rolled open-addressing table.

---

## Summary table

| # | Opportunity | Severity | Effort | Lands in |
|---|---|---|---|---|
| 1 | JIT: bail out instead of silent `null` | **Correctness — wrong answers today** | Hours | `crush-jit/src/compiler.rs:421` |
| 2 | Conformance corpus + runner (4 engines) | Highest leverage | Days | `tests/conformance/`, `xtask/` |
| 3 | Wire source locations through the AST | Language quality | Days | `crush-cast`, `crush-frontend`, `casm`, `crush-vm` |
| 4 | Lambdas unparseable (lexer bug) | Feature is a lie | Hours *(closures: a wave)* | `crush-frontend/src/parser/lexer.rs:700` |
| 5 | Fuzz the parser (`cargo-fuzz`) | Prevents the cson bug class | Hours → days | new `fuzz/` |
| 6 | Differential-test the four engines | Finds #1's class automatically | Days | `fuzz/`, `xtask/` |
| 7 | Memory model: cycles leak; GC unreachable | Design decision | A wave | `crush-vm/src/vm.rs`, `memory.rs` |

**If only one thing ships: #1** (hours, and it is shipping wrong answers).
**If only one thing ships that changes the trajectory: #2** — it is the mechanism that would
have caught #1, #3, #4 and the crush-cson bugs, and it is what stops the "both ends, no middle"
pattern from recurring.
</content>
