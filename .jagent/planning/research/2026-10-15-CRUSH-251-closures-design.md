# CRUSH-251 slice 1 — function values and closures (design, draft 1)

Status: **draft for review** (vega ↔ foreman), 2026-10-15. Owner decisions that frame it (#185):
closures come before #175 `parallel_map`; frontends stay parser-only (CRUSH-47 declined); AOT
backends are **exact or loud**, never silently wrong.

## Why

Blocks: Python `lambda` and nested-`def` closures, JS closures/arrow functions that capture, any
higher-order function (`map(f, xs)`, callbacks, `sort(key=...)`), and #175 `parallel_map(xs, |n| ..)`.

## Today (main `0360dc3`)

- CAST has `Expression::Lambda { params, body }`. The compiler **lambda-lifts** it into a named
  function (`__lambda_N`, or the `let` name as a hint) and the lambda's *value* is `push_str "<name>"`.
- Captures are rejected (`compiler.rs check_lambda_capture`: "lambdas can't capture variables yet").
- Calling through a variable is rejected (`compiler.rs ~1486`) unless the variable *is* the lifted
  name (`let f = |x| ..; f(1)` works because the lambda was lifted *as* `f`).
- VM: `CALL` takes a static function index. `SPAWN` already resolves a function **by name at runtime**
  (`func_entry`), so dynamic dispatch exists in principle.
- AOT Rust/C: `call` emits a direct call to a sanitized static symbol.
- Python walker: `lambda` → "not yet supported"; nested `def` → plain `FunctionDef` (no capture).
  JS walker: arrow/function expressions → CAST `Lambda`.

## Proposal

### 1. A function value

New `Value::Func(Rc<FuncValue>)` with `FuncValue { name: String, captured: Vec<Value> }`.
- A named function used as a value (`apply(fib, 3)`) → `Func { name: "fib", captured: [] }`.
- A lambda → `Func { name: "__lambda_N", captured: [..] }`.
- Printing: `<fn fib>` (language-styled printers render their own, e.g. Python `<function ...>`
  only if a corpus needs it — otherwise loud/unspecified).
- Equality: identity of name + captured (not needed by any corpus; keep simple).

Rationale: one variant, cheap to clone (Rc), carries everything a call needs; keeps the existing
name-based function table as the code pointer.

### 2. Capture semantics: **by reference, via boxes, only where needed**

Python and JS closures see later writes and can write captured variables
(`counter()` incrementing an outer `c`). So capture is by reference — but most captures are
read-only, and copying is cheaper and simpler. Compiler rule:
- A variable captured by some lambda **and** assigned anywhere after its declaration (in the outer
  function or in any lambda) is **boxed**: stored as a 1-slot cell (`Value::Array` of len 1 reused as
  the box, or a dedicated `Value::Cell`), every load/store of it in every scope goes through the box.
- Other captured variables are copied into `captured` at closure creation (by value).
- The lifted function gets the captures as **leading hidden parameters** (`__cap_x`), so inside the
  body they are ordinary locals/params — no new "upvalue" opcode in any engine.

Language notes:
- Python late binding (`[lambda: i for i in range(3)]` → all see the last `i`) falls out of boxing `i`.
- JS `for (let i ...)` creates a **fresh binding per iteration** — the JS walker must give each
  iteration its own variable (copy into a per-iteration local the closure captures). `var` shares one.
- Rust closures: `move` = by value; borrowing closures that mutate (`FnMut`) = boxed. Out of scope
  for slice 1 unless trivial.

### 3. New ops (CASM + CVM1)

- `MAKE_CLOSURE name ncap` — pops `ncap` captured values, pushes `Func { name, captured }`.
  (`ncap = 0` replaces today's `push_str "<name>"` lambda value.)
- `CALL_VALUE argc` — pops `argc` args and the callee; callee must be `Func` (or, for
  compatibility, a `Str` naming a function — today's lambda representation and `spawn`'s); pushes
  `captured ++ args` and calls the named function. Arity mismatch → runtime error.
- Compiler: a call whose target is a **variable/parameter** (not a known function) → `CALL_VALUE`;
  a call to a known function stays `CALL` (no cost for the common case). A bare function name used
  as a value → `MAKE_CLOSURE name 0`.
- Boxes: `box_new`, `box_get`, `box_set` — or reuse `new_array 1` / `arr_get` / `arr_set` with
  index 0 (zero new opcodes; slightly slower). Prefer reuse for slice 1.

### 4. Engines

| engine | slice 1 |
|---|---|
| scheduler (native VM) | full: `Func`, `MAKE_CLOSURE`, `CALL_VALUE` (dispatch via `func_entry`) |
| portable VM (crush-web) | full, same dispatch |
| AOT Rust | `RuntimeValue::Func`; `CALL_VALUE` → a generated `match name { .. }` over all program functions |
| AOT C | `TAG_FUNC` + a generated dispatch switch over function ids; captured values in an array |
| FastVM / JIT | **loud**: reject `MAKE_CLOSURE`/`CALL_VALUE` at load ("closures not supported on FastVM/JIT yet") |

AOT Rust and C are realistic in slice 1 because the dispatch is a closed `match` over the program's
own functions (no function pointers needed). If that slips, they must reject at compile time.

### 5. Frontends

- Crush source: `|x| expr` lambdas already parse; captures now allowed; calling a variable allowed.
- JS: arrow / function expressions → `Lambda` (exists); calls through variables now compile; per-
  iteration `let` bindings in `for` loops (see §2).
- Python: `lambda` → `Lambda`; nested `def` → `Lambda` bound to the name (so it captures);
  `map`/`filter`/`sorted(key=)` builtins can follow once function values work.
- Rust: closures `|x| ..` → `Lambda` (non-`move` borrowing that mutates: out of scope / loud).

### 6. Tests (definition of done for slice 1)

- Corpus programs on every frontend that has closures: counter (mutating capture), adder factory
  (read-only capture), late binding in loops (Python) vs per-iteration (JS `let`), passing a named
  function as a value, recursion through a closure variable, closures stored in arrays/maps.
- `run_both`-style parity on scheduler + portable VM; AOT exact-or-loud like `python_parity`.
- Python `cpython_corpus` gains `14_closure` back; JS/rust corpora gain their cases.

## Open questions for review

1. **Box representation**: reuse 1-element arrays (zero new ops, but a box prints/compares like an
   array if it ever leaks) vs a dedicated `Value::Cell` + 3 ops?
2. **Hidden leading params** for captures vs a `captured` slot read via a new `LOAD_CAPTURED i` op
   (avoids changing arity, keeps stack traces/param counts honest)?
3. **AOT in slice 1** (generated dispatch) — or ship VM-only first with AOT rejecting loudly?
4. **`Str` callee compatibility** in `CALL_VALUE`: keep (spawn/lambda back-compat) or require `Func`?
5. **Escape analysis precision**: box only variables both captured and mutated (proposed) vs box
   every captured variable (simpler, slower, fewer chances to get it wrong)?
6. **GC/cycles**: closures in boxes can form `Rc` cycles (a closure capturing a box that holds
   itself) → leak, not unsoundness. Acceptable for slice 1?
7. Anything in CRUSH-121 (GC root set) that a `Func` value must register with?
