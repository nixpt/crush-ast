# CRUSH-251 slice 1 — function values and closures (design, draft 2)

Status: **draft 2, reviewed** (vega wrote draft 1; foreman critiqued it; this folds the critique in),
2026-10-15. Framing owner decisions (#185): closures before #175 `parallel_map`; parser-only
frontends (CRUSH-47 declined); AOT backends **exact or loud**.

## Why

Blocks Python `lambda` / nested-`def` closures, JS closures that capture, higher-order functions
(`map(f, xs)`, callbacks, `sorted(key=)`), and #175 `parallel_map(xs, |n| ..)`.

## Today (main `0360dc3`) — including two silent bugs slice 1 must fix

- CAST `Expression::Lambda` is **lambda-lifted** to a named function — `__lambda_N`, or **the `let`
  name** — and its value is `push_str "<name>"`. Captures are rejected (`check_lambda_capture`);
  calling through a variable is rejected unless the variable *is* the lifted name.
- **Bug A — same-named lambdas overwrite each other** (`self.lambdas.insert`):
  `fn a(){ let h=|x| x+1; return h(10) }  fn b(){ let h=|x| x*100; return h(10) }`
  prints `11 11` (expected `11 1000`).
- **Bug B — a `let`-bound lambda replaces a top-level fn of the same name program-wide**:
  `fn f(x){ return x+1000 }  fn g(){ let f=|x| x*2; return f(5) }  print(g()); print(f(1))`
  prints `10 2` (expected `10 1001`).
- VM `CALL` is static; `SPAWN` resolves a name at runtime (`fn_name = pop!().as_text()`).
  AOT Rust/C emit direct calls to static symbols. Python walker rejects `lambda`; nested `def` →
  plain `FunctionDef`. JS arrow/function expressions → `Lambda`.
- `casm_to_vm`'s `slot_map` is **name → slot per function**: shadowed / block-scoped bindings with the
  same name share a slot.

## Design

### 1. Lifting and naming
Every lambda lifts to a **unique** `__lambda_N`; the binding name is kept only as metadata for display
and stack traces (`<fn h>`). A `let`-bound lambda is an ordinary variable holding a `Func`, called via
`CALL_VALUE`. (Later optimisation: a never-reassigned binding to a capture-free lambda may use a
direct `CALL`.) This fixes bugs A and B. Nothing may emit a direct `CALL` to a function that takes
hidden capture params.

### 2. Values
- `Value::Func(Rc<FuncValue>)`, `FuncValue { name: String, ncap: u16, captured: Vec<Value>, display: String }`.
  A named function used as a value is `Func { name, captured: [] }`.
- `Value::Cell(Rc<RefCell<Value>>)` — the box for by-reference captures (**Q1: dedicated Cell**, not a
  1-element array: an array box leaks `len`/iteration/printing/JSON/equality/marshaling semantics; a
  Cell makes any path that forgets to unbox fail loudly).
- Equality: `Func` by name + captured, with Cells compared by **identity** (`Rc::ptr_eq`).
- Printing / type names: settled per frontend (JS `typeof` → `"function"`, Python `type()` →
  `function`); in the language-styled printers (`py.print`, `conv.text`) printing a `Func` is **loud**
  until a corpus pins the exact format.

### 3. Capture semantics — copy only when provably safe, otherwise box
**Rule (Q5):** a captured variable is **copied** into `captured` iff it has **exactly one assignment
and that assignment dominates every closure creation that captures it**; every other captured
variable is **boxed** (a `Cell`, with every load/store of it in every scope going through the cell).
- "Assignment" includes `for` targets, augmented assignment, Python `with … as` / `except … as` /
  walrus (`:=`) and `del`.
- This boxes self-reference correctly: `let fact = |n| … fact(n-1)`, mutually recursive nested
  `def`s and JS hoisted function declarations capture a variable *before* its initialising
  assignment (a by-value copy would capture null). If dominance gets hairy, fall back to box-all.
- **Transitive capture:** a variable used by an inner lambda two levels down is captured by every
  intermediate lambda (as a hidden param) so it can be passed down (`make_adder(a) → |b| → |c| a+b+c`).
- Python late binding (`[lambda: i for i in range(3)]`) falls out of boxing `i`.

### 4. Scoping — binding identity before capture analysis
Capture analysis keys on **bindings**, not names, so the walkers (or a compiler pre-pass)
**alpha-rename** first:
- **JS:** `let`/`const` block-scoped, `var` hoisted to function scope; `for (let i …)` gets a **fresh
  cell per iteration**, carrying the previous value over before the update (`i++`).
- **Python:** an assignment in a nested `def` makes the name **local** unless declared `nonlocal`
  (`c += 1` without `nonlocal` is `UnboundLocalError`, not a capture); `global`; comprehensions have
  their own scope.
- **Crush source:** lexical, as today.

### 5. Ops (CASM + CVM1)
- `MAKE_CLOSURE name ncap` — pops `ncap` values, pushes a `Func`.
- `CALL_VALUE argc` — pops args and callee; callee **must be a `Func`** (**Q4**: no `Str` callees —
  otherwise any string naming a function becomes callable; JS `"main"()` must be a TypeError);
  pushes `captured ++ args`, calls `name`. Arity is checked against the user-visible arity
  (`params − ncap`).
- `CELL_NEW`, `CELL_GET`, `CELL_SET`.
- Compiler: calls to known functions stay `CALL`; a bare function name used as a value emits
  `MAKE_CLOSURE name 0`.
- **Captures as hidden leading params (Q2)**, with `ncap` recorded in function metadata so the arity
  check, error messages, stack traces and the debugger show the user's arity.

### 6. SPAWN and #175
`SPAWN` must accept a `Func` (CAST `Spawn { function: String }` and the scheduler's
`pop!().as_text()` would turn it into `"<fn …>"`); the `Str` form stays only in `SPAWN`, for
back-compat. #175 `parallel_map` runs closures in **share-nothing worker VMs** (#185): a capture
holding a `Cell` (an `Rc`) can't cross into a worker — `parallel_map` either deep-copies captures (and
documents that writes don't come back) or rejects boxed captures loudly. Decide in #175.

### 7. Everywhere locals are read by name must unbox
Not only `LOAD`: polyglot marshaling (`prepare_polyglot_blocks`: free vars ∩ known locals feed the
`@python`/`@js` prologue), the debugger's locals and watch views, `export_var`, and spawn args.
Each must unbox a `Cell` (another reason a Cell beats an array box: forgetting fails loudly).

### 8. Engines (Q3: VM first)
| engine | slice 1 |
|---|---|
| scheduler (native VM) | full |
| portable VM (crush-web) | full |
| AOT Rust / AOT C | **reject at compile time** (`MAKE_CLOSURE`/`CALL_VALUE`/`CELL_*`: "closures are not supported by the AOT backends yet") |
| FastVM / JIT | reject at load |

When AOT C gains closures, its mark-and-sweep GC **must trace** `TAG_FUNC` captured values and Cells
— missing that is a use-after-free (the CRUSH-121 root-set bug class), not a leak.

### 9. Memory (Q6, Q7)
- Rc cycles through Cells (a closure capturing a cell that holds itself) leak in the Rc VMs —
  acceptable; same class as `a.push(a)` today. Note in the ticket. AOT C's mark-and-sweep doesn't leak
  as long as it traces closures.
- CRUSH-121 (FastVM arena GC, roots = current stack + locals): nothing for slice 1 (FastVM rejects
  closures). When FastVM gains them, `Func.captured` and Cells must be traced and frames holding
  hidden capture params must be roots — add that line to CRUSH-121.

## Definition of done (slice 1)
- **Bugs A and B fixed**, both programs in the corpus.
- Corpus programs per frontend with closures (Crush, JS, Python; Rust if trivial): counter (mutating
  capture), adder factory (read-only), transitive capture (`a → b → c`), self-recursive closure,
  mutually recursive nested defs, Python late binding vs JS per-iteration `let`, a named function
  passed as a value, closures stored in arrays/maps, `nonlocal` vs local in Python.
- `run_both` parity on scheduler + portable VM; AOT/FastVM/JIT reject loudly (tested).
- Python `cpython_corpus` gets `14_closure` back.
- Unboxing at every by-name local reader (§7), tested.

## Order of work
1. Unique lifting (fixes A, B) — small, independent, can land first.
2. `Func` + `MAKE_CLOSURE` (ncap 0) + `CALL_VALUE` (no captures yet): function values work.
3. Captures: alpha-renaming, dominance rule, transitive capture, `Cell` + ops, unboxing sites.
4. Frontends: Crush, JS (per-iteration `let`), Python (`lambda`, nested `def`, `nonlocal`).
5. SPAWN takes `Func`; loud rejections on AOT/FastVM/JIT; corpus.
