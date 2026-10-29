# CRUSH-293 — boa walker: spread call arguments (`f(...args)`)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-293 |
| **Priority** | P1 (maya blocker — the next item in the CRUSH-276 queue) |
| **Status** | Partially resolved (D2 #299 and D3 #300 merged; variadic function-value ABI follow-up open) |
| **Phase** | COSM K3 follow-up (boa walker) |
| **Assignee** | freebuff (dispatched by muse) |
| **Dependencies** | CRUSH-276 (queue), CRUSH-zorro-18 (the rest/default ABI), CRUSH-251 (function values — only for design D3), CRUSH-111 (intrinsic table contract) |
| **Estimated effort** | S–M, in two independent slices (D2 walker-only, S; D3 opcode, M) |
| **Filed by** | muse — 2026-10-22, from the boa refusal inventory (investigation by freebuff under muse dispatch) |

## Problem

The boa walker refuses every spread in an argument list, at walk time,
before anything is lowered:

```js
f(...xs)              // plain call, spread last
f(...xs, 3)           // spread first
f(1, ...xs, 3)        // spread middle
f(...xs, ...ys)       // several spreads
o.m(...xs)  this.m(...xs)  a.m(...xs)
new C(...xs)          super(...xs)  super.m(...xs)
console.log(...xs)
```

all fail with the same message (`crates/crush-lang-js/src/lower/expr.rs`):

- `Lower::args` (expr.rs:621–630, message at :625) — every call that passes
  raw arguments;
- `Lower::adapt_args` (expr.rs:954–965, message at :964) — every call whose
  target signature the walker knows.

The swc lowering refuses it identically (`lower_swc.rs:1933–1939`) and stays
frozen (CRUSH-250 step 6 retires swc for `.js`), so this ticket is boa-only.

Maya-engine's decorator/util code uses the wrapper shape
`function (...args) { … fn(...args) … }`, which is why this sits next in the
CRUSH-276 queue. (The maya file count was **not** re-measured here — the
maya-engine tree is outside this worktree; the queue position is muse's
dispatched finding, not a number this ticket stands behind.)

## Evidence

All probes ran through the crate's own pipeline —
`JsFrontend::with_lowering("js", Boa)` → `frontend_pipeline` →
`Compiler::compile` → `casm_to_vm` → `run_with_caps` — i.e. exactly what
`loud_constructs_test.rs`'s `run_crush` does, with the lowering switched to
boa. Scratch probe files were deleted; the commands were
`cargo test -p crush-lang-js --test <probe> -- --nocapture`.

### 1. Current behaviour, per shape

| shape | outcome | message (verbatim, `{:#}`) |
|---|---|---|
| `f(...xs)` spread last | walk refuse | `a spread argument (\`f(...args)\`) is not supported by the JS walker yet (line 1, column 69)` |
| `f(...xs, 3)` spread first | walk refuse | same message |
| `f(1, ...xs, 3)` spread middle | walk refuse | same message |
| `f(...xs, ...ys)` | walk refuse | same message |
| `f(...[1, 2])` literal | walk refuse | same message |
| `console.log(...xs)` | walk refuse | same message (`Arity::Any`, so the arity check passes first) |
| `Math.max(...xs)` | walk refuse, **wrong message** | `` `Math.max()` with 1 argument(s) is not supported by the JS walker yet `` — the row is `Arity::Exactly(2)` (`builtins.rs:95`) and the arity check runs before the spread check (expr.rs:1047 for the arity check, :1052 for `self.args`); a spread counts as one argument |
| `a.m(...xs)`, `a` a `new A()` instance | walk refuse | the spread message |
| `this.m(...xs)` in a method | walk refuse | the spread message |
| `o.m(...xs)`, `o` an object literal | walk refuse, **different blocker** | `the method \`.m()\` (a method call on this value waits for CRUSH-277/zorro-44)` — member resolution fails before the arguments are looked at |
| `new C(...xs)` | walk refuse | the spread message (`class.rs:489`) |
| `super(...xs)` | walk refuse | the spread message (`expr.rs:230`) |
| `super.m(...xs)` | walk refuse | the spread message (`class.rs` instance/static branches) |
| `a.push(...b)` (statement) | walk refuse | the spread message (the `Push` row lowers through `Lower::args`) |
| `f(...s)`, `s` a string | walk refuse | the spread message — **no type check at walk time** |
| `f(...arguments)` | walk refuse | the spread message fires first; `arguments` alone is `` `arguments` as a value (CRUSH-zorro-18 (`arguments`)) ``, so that shape stays out of scope |
| `h.apply(null, xs)` | walk refuse | `the method \`.apply()\` (a method call on this value waits for CRUSH-277/zorro-44)` — no apply precedent in the table |

### 2. What already works around the refusal (so spread is the *only* gap)

| program | result |
|---|---|
| `function f(a,b){return a+b} const g = f; console.log(g(1,2))` | `3` |
| `… console.log(g(1))` | runs, then `function \`f\` takes 2 argument(s), got 1` (runtime arity check on the value path) |
| `function callit(f){return f(1,2)} function add(a,b){return a+b} console.log(callit(add))` | `3` |
| `twice(f, x)` with a function expression / arrow argument | `3` |
| `function f(a,b,c){…} console.log(f(1,2,3))` (extra arg) | `3` — extras dropped, as JS ignores them |
| `console.log(1, 2, 3)` | `1 2 3` |
| `a.push(2, 3)` (statement) | pushes both |
| `function f(a, rest){return rest.length} f(1, xs)` | `2` — a rest landing site is literally one array argument |
| crush `let c = a + b` with two arrays | `3` — the VM's `+` concatenates arrays (CRUSH-135) |

So function values, callbacks, value-callee arity checks and variadic
capabilities all work; a spread argument has no representation to lower *into*.

### 3. Pinning today

- `crates/crush-lang-js/tests/node_corpus/arr_spread_call.js` (spread into a
  3-parameter function, `.out` = `6`) is in `node_corpus_test.rs`'s
  `KNOWN_LOUD` list, which pins *loudness* on both lowerings but not a
  message.
- `loud_constructs_test.rs:70` pins the message text, but that file is
  `#![cfg(feature = "ts")]` and runs through the swc lowering only.
- `boa_backend_test.rs`'s `loud()` helper has no spread case.

## Layer findings

### CAST — fixed-length argument lists only

`crush-cast/src/lib.rs`: `Call { function: String, args: Vec<Expression> }`
(:349), `CapabilityCall` (:355), `MethodCall` (:363), `Construct` (:407),
`Spawn` (:381). Every call node carries a **fixed** argument list; nothing
variadic, no `apply`/arg-array node. (`KindValue::Apply` at :553 is COSM kind
metadata — `value(copy)` — unrelated.)

There is no `__crush_*` intrinsic for a call with a runtime arg array either:
`crates/crush-frontend/src/walker_intrinsics.rs`'s `lookup` table (:64–92)
has no such entry and `Intrinsic::validate` (:27–38) enforces an exact
`argc` for all of them.

### crush-frontend — arity is a compile-time property

`Expression::Call` (compiler.rs:1771ff) pushes the arguments last-first and
emits `call { function, argc }`; `call_value { argc }` when the callee is a
declared variable (:1812–1823); `construct { kind, argc }` (:1760–1770);
`call_member { name, argc }` (:2310, :2433); an unknown *dotted* name becomes
`cap_call { name, argc }` with a permission grant (:2455–2462) — so a
hypothetical `js.apply` written as a `Call` would become a **host
capability**, which cannot re-enter the VM to invoke a crush function.

Arity itself is checked statically: `semantics.rs:533`
(`Function '{}' expects {} arguments, found {}`). A hand-written
`fn f(a, b) … f(1)` in crush is refused at compile time, so a runtime-length
argument list has no way through the checker as-is.

### CASM — `call` carries no argument count at all

`OpCode::Call(String)` (`casm/src/lib.rs:122`) — and the assembler drops the
JSON `argc` for it (:444–448). Contrast `CapCall { name, argc }` (:174),
`CallMember` (:259), `Construct`, `CallValue` (u16 argc). So for a direct
call the operand stack is the entire calling convention: the callee's prologue
is one `store` per parameter, in order (`compiler.rs:415–421`), and the
*caller* decides how many values it pushes.

### The VM — the direct call path never validates arity

- `CALL` (`portable_vm.rs:1124–1145`): pops nothing, resolves the entry,
  pushes a frame, jumps — "Arguments stay on the stack (main VM convention)".
- `CALL_VALUE` (`portable_vm.rs:1182–1199`): the **only** call path with a
  runtime arity check — `vm::callable(&callee, argc)` (`vm.rs:179–190`,
  `FuncArity`), because `Value::Func` carries `arity`.
- `CALL_MEMBER` (:1516–1524 → `cosm_rt::call_member`, `cosm_rt.rs:826–851`)
  and `CONSTRUCT` (:1525–1545) both take `argc` from the instruction and check
  it against the target.
- Rest parameters are *received* as one array: the caller builds an
  `ArrayLiteral` from the trailing arguments (`adapt_args`'s `Param::Rest`
  branch, expr.rs:994–1008) and the callee binds one plain parameter
  (`Param::Rest`, `lower/scope.rs:60–64`). The inverse (an array expanded into
  slots) is not representable — the stack ABI has no "push-all" operation.

Array reads past the end are loud, not `null`: `ArrayBounds`
(`portable_vm.rs:2511–2523`, `scheduler.rs:1942–1946`), verified live —
`crush-run run` on `let a = [1, 2]; print(a[5])` prints
`[runtime] array index out of range: 5 (len 2)`.

### AOT backends — fixed C/Rust signatures, no function values

`crush-aotc/src/codegen.rs:402–422` emits a direct C call with a
compile-time-known argument list (`mangle(func_name)(args…)`); arity is fixed
by the instruction. `make_closure` / `call_value` bail (:513), and the
COSM member/construct ops bail (:504). Unknown ops reach the catch-all that
writes `/* unhandled op: … */` and continues — so **any new opcode must be
added to the explicit bail list**, or AOT C silently computes null.
AOT Rust behaves the same (`crush-aot/tests/function_values_loud.rs`).
FastVM: `FastOp::Call` (`fastvm/execution.rs:113–145`) takes `argc` from the
instruction and records `locals_count: argc` in the frame; JIT builds on the
FastVM program (`crush-jit/src/compiler.rs:145, 263`), and `lower_program`
refuses `call_value` (same test file). So dynamic argc needs a new FastVM
instruction plus a JIT story, or a loud refusal.

### Existing "apply" precedent: none

- `js.*` capabilities are `div`, `add`, `truthy`, `loose_eq`, `format`
  (`caps.rs:78–118`; `portable_vm.rs:2023–2029`, `scheduler.rs:2113–2119`).
  `js.format` is the only variadic one (`argc: None`, `caps.rs:106–110`) and
  it takes its varargs **on the stack**, not as an array.
- `Function.prototype.apply` is not in the builtin table; it is refused as an
  unknown method on a non-name receiver.
- The closest machinery is CRUSH-251's function values: `FuncValue { arity }`
  + `CALL_VALUE`'s runtime arity check. Nothing today can pass *an array* as
  the argument list.

## Candidate designs

### D1 — literal/known-length unrolling (walker only, **S**)

Rewrite `f(…, ...[a, b], …)` into `f(…, a, b, …)`; any spread source whose
length isn't a literal stays refused. Cannot touch `f(...xs)`.

- Covers: every shape (plain, method, `new`, `super`, builtins) — it is a pure
  argument-list rewrite before the existing dispatch.
- Changes: `lower/expr.rs` only (+ tests).
- Cannot do: the runtime-length case (`f(...xs)`), i.e. the common one.
- Risk: none beyond evaluation order (element order is preserved naturally).

### D2 — index expansion at a known signature (walker only, **S/M**) ← recommended first

When the walker already knows the callee's parameter list (the places that
call `adapt_args` today: `function` declarations, `this.m`, a kind-tracked
`a.m`, `new C`, `super`, `super.m`, `C.static`) and **every parameter is
plain**, lower `f(...xs)` into `f(xs[0], …, xs[N-1])` with N = `sig.len()`:

- JS still evaluates the spread source once and reads its elements in order;
  N positional reads preserve observable order and values.
- A *longer* array is unobservable: JS ignores the extras and so does the
  caller-side lowering (a fixed call already drops extra args).
- A *shorter* array is JS's `undefined` argument, which crush does not have
  (CRUSH-zorro-12): it becomes a loud `array index out of range` error. That
  is the house rule (loud, never silently wrong) and must be in the tests.
- Guard: the spread source must be an `Identifier` or an array literal, so
  the source can't be re-evaluated N times (`f(...makeArray())` stays
  refused); a literal source is D1.
- Guard: any `Param::Default`/`Param::Rest` in the signature keeps the
  current refusal (their hidden "given" flags / single-array slot cannot be
  produced from a runtime length).

- Covers: plain `f(...xs)` where `f` is a declaration in this module, and the
  same for methods/constructors/statics the walker already resolves. Not:
  value callees (`fn(...args)` — the maya decorator shape), builtin rows
  (`console.log(...xs)`, `Math.max(...xs)`), `o[k]()` and other shapes
  refused before the arguments are reached.
- Changes: `lower/expr.rs` + the `class.rs` call sites, ~one shared helper;
  no CAST/CASM/VM/AOT change at all.
- Semantics notes: reads happen in argument order, so an earlier argument
  that reassigns the spread source (`f((xs = y), ...xs)`) still sees the new
  array, as JS does. And the source is assumed *array-like*: an object with a
  `length`, or a string whose indexing is byte- rather than code-point-based,
  would either read the same way or fail loudly — it is not JS's iteration
  protocol, and that should be a line in the ticket that lands it.
- Risk: the "too few elements" divergence above, and that array-like
  assumption.

### D3 — `__crush_apply__(callee, argsArray)` + a `call_value_apply` opcode (**M**, VM-only)

A new intrinsic with a *fixed* CAST arity of 2 (so the checker and the
CRUSH-111 table contract are satisfied), compiled into a new CASM op that:
pops the array, pops the callee, checks `Value::Func`, checks
`arity == len`, pushes the elements as arguments and enters the frame — the
same frame setup `CALL_VALUE` already does (`portable_vm.rs:1182–1199`).

- Covers: value callees — `fn(...args)` inside a wrapper/decorator, which is
  precisely the maya shape — and, for free, direct calls to plain-parameter
  functions (lower `f(...xs)` to `apply(Var f, xs)`).
- Not: methods/`super`/`new` (each needs its own member/construct variant),
  builtin rows, callees with default/rest parameters (the walker already
  refuses to use those as values, `expr.rs:40–52`), and `arguments`.
- Changes: `walker_intrinsics` entry + compiler arm; CASM opcode + codec;
  `crush-lang-sdk/src/compile.rs`'s CASM→assembly lowering (:1206ff is the
  `call_value` pattern); `PortableVm` + scheduler arms; FastVM lowering and
  the JIT refuse (as `call_value` already does); AOT C/Rust bail (add to the
  bail list — the catch-all would otherwise emit a silent null); the
  `casm_lowering_completeness` test enumerates every opcode
  (`crush-lang-sdk/tests/casm_lowering_completeness.rs:212–213`).
- Semantics to pin: non-array second argument → new loud error; non-function
  callee → existing `NotCallable`; length ≠ arity → existing `FuncArity`
  (loud, and stricter than JS, which tolerates both directions).
- Implementation variant worth knowing: `parallel_map` shows the VM can run
  a crush function from the cap path (`portable_vm.rs:1106–1116`,
  `parallel.rs`), so a `js.apply` capability could avoid the new opcode. But
  that makes a *language-level* operation a declared, gated permission, which
  is off-pattern for the capability model — prefer the opcode, and note that
  the AOT bail is needed either way (AOT C's `cap_call` arm silently emits
  null for capabilities it doesn't know).
- Risk: it is a VM-lane feature, so a program that uses it cannot be
  AOT-compiled — acceptable and precedented (`call_value`, CRUSH-251), but it
  is a *new* loud refusal on the AOT path for programs that today fail
  earlier in the walker.

### D4 — the rest ABI in reverse (walker only, **S**, very narrow)

For a callee whose signature ends in `...rest` and whose spread is the
trailing argument group, pass the array into the rest slot directly. It needs
a *copy* (JS rest arrays are fresh), e.g. a `js.array_copy` capability or the
VM's array `+` (`[] + xs`, CRUSH-135) — the latter works but is a hack.
Covers only rest-parameter callees; folds into D2's guard as a later
increment rather than a design of its own.

### D5 — keep refusing everything but literals (D1 alone, **XS**)

The fallback: no VM change, no risk, and `f(...xs)` keeps failing loudly.

### Coverage matrix

| shape | D1 | D2 | D3 | D4 |
|---|---|---|---|---|
| `f(...xs)`, `f` a declaration with plain params | ✗ (runtime) | ✓ | ✓ | ✗ |
| `f(a, ...xs)`, mixed fixed + spread, runtime length | ✗ | ✓ | ✓ | ✗ |
| `fn(...args)`, `fn` a value/parameter (decorator) | ✗ | ✗ | ✓ | ✗ |
| `this.m(...xs)` / kind-tracked `a.m(...xs)` | ✗ | ✓ | ✗ | ✗ |
| `new C(...xs)` | ✓ (literal) | ✓ | ✗ | ✗ |
| `super(...xs)` / `super.m(...xs)` | ✓ (literal) | ✓ | ✗ | ✗ |
| `console.log(...xs)`, `Math.max(...xs)` | ✓ (literal) | ✗ | ✗ | ✗ |
| callee with default/rest params | ✗ | ✗ | ✗ | partial |
| runs on AOT / FastVM / JIT | ✓ | ✓ | ✗ (loud) | ✓ |

## Recommendation

**Ship D2 first**, as its own PR: it is walker-only, it unblocks every shape
the walker already resolves at the *actual* runtime length, it needs no CAST,
CASM, VM or AOT change, and the corpus already has the fixture
(`arr_spread_call`, node `6`). Smallest useful slice:

- `f(...xs)` / `f(a, ...xs)` / `f(...xs, a)` / several spreads, where the
  target signature is known and all parameters are plain (`function`
  declarations first if the whole matrix is too much for one PR);
- source must be an identifier or array literal; non-identifier sources stay
  refused;
- signatures with default/rest parameters stay refused;
- a short array is a loud `array index out of range` (documented divergence,
  not a silent wrong value).

Also in that PR: check for a spread *before* the row arity check in
`Lower::builtin`, so `Math.max(...xs)` reports the spread (the real blocker)
instead of `with 1 argument(s)`.

**Then D3** as the follow-up for the maya decorator shape, scoped to value
callees only, VM-lane, with AOT/FastVM/JIT refusing loudly and a test that
says so.

Out of scope, each needing its own ticket if ever wanted: `console.log(...xs)`
(needs a `js.format`-style array-splat capability), `arguments` (zorro-18),
array spread in literals (`[...a]`, zorro-23/16), `o[k](...)`, and
`Function.prototype.apply`.

D3 settles the arity question strictly: `call_value_apply` uses the same
exact-arity check as `call_value`; both too few and too many arguments fail
with `FuncArity`. This intentionally differs from JS, which ignores extra
arguments.

## Acceptance criteria

D2 (not part of this resolution; remains open):

- [ ] `function f(a,b,c){return a+b+c} const xs=[1,2,3]; console.log(f(...xs))`
      → `6`; `node_corpus/arr_spread_call` moves from `KNOWN_LOUD` to matching
      and is added to `BOA_FIXED`.
- [ ] spread first / middle / last / two spreads:
      `g(...xs, ...ys)` → `10`, `g(...[], ...xs, 30, 40)` → `73`
      (node-verified), and `f(1, ...[2], 3)` → `6`.
- [ ] `new B(...xs)` → `3`; `class C extends B { constructor(p){ super(...p) } }`
      → `new C([5,6]).n` is `11`.
- [ ] method and `this.m`: `a.m(...xs)` → `3`, `sum(xs){ return this.m(...xs) }`
      → `3`.
- [ ] loud-stays-loud, with the message pinned in `boa_backend_test.rs`:
      a spread into a signature with a default or rest parameter; a spread
      whose source is not an identifier/literal; `console.log(...xs)`;
      `f(...arguments)`.
- [ ] a short array is loud: `function f(a,b){…} f(...[1])` → a runtime
      `array index out of range` error, not a silent value.
- [ ] `Math.max(...xs)` names the spread, not the arity.
- [ ] swc's behaviour is unchanged (its `loud_constructs_test` case stays).

D3 bounded slice (implemented here; not the generic decorator follow-up):

- [x] `relay(fn, xs) { return fn(...xs) }` with `relay(add, [1,2])` → `3`,
      plus a direct value-callee spread, run on scheduler and PortableVm.
- [x] `apply` of a non-array, of a non-function, and of an arity mismatch each
      fail loudly with the pinned VM errors.
- [x] AOT C, AOT Rust, FastVM/JIT refuse the opcode loudly, and
      `casm_lowering_completeness` covers it.
- [x] The opcode is on AOT C and Rust's explicit refusal paths.

## Resolution (D2, 2026-10-22)

Implemented as recommended (freebuff under muse dispatch; reviewed
and verified by muse). `Lower::adapt_args` routes any argument
list containing a spread to `Lower::spread_args`: with an all-plain
signature, literal spreads splice as elements and name spreads are
modelled as one concatenated argument array (ordinary arguments as
array-literal segments, crush `+` concatenates) read back by slot —
`f(...xs)` becomes `f(xs[0], …, xs[N-1])`. Pieces next to a runtime
spread must be side-effect free (each slot read re-evaluates the
model; pure pieces make that unobservable). Short arrays fail
loudly at run time (`array index out of range`); default/rest
signatures, non-name sources, unknown-signature call sites
(`console.log(...xs)`, value callees) stay refused, as does
`f(...arguments)`. `Math.max(...xs)` now names the spread instead
of the row arity. `arr_spread_call` moved from KNOWN_LOUD to
BOA_FIXED in the corpus harness, with AOT parity allow-listed as
before. D3 (value-callee apply opcode) remains open above.

## Resolution (D3, 2026-10-22)

Implemented the bounded D3 apply path: Boa lowers a spread-only call through a
value callee to `__crush_apply__(callee, argsArray)`, a fixed two-arg walker
intrinsic and `call_value_apply` CASM/CVM1 opcode. Both CVM1 execution engines
enforce strict function arity; invalid callee, argument-array type, and arity
mismatch are tested. AOT C/Rust and FastVM (and therefore JIT) reject the
opcode explicitly. Boa e2e tests exercise `relay(fn, xs)` and a direct
value-callee spread on both CVM1 engines.

The generic decorator case remains blocked and is intentionally not part of
this D3 slice: a rest/default-parameter function used as a value depends on a
variadic function-value ABI, while `CALL_VALUE`/`call_value_apply` retain the
exact-arity contract and the walker refuses those function values. Do not
expand that ABI here. Track this as the follow-up slice: **variadic
function-value ABI for rest/default functions**, including decorator wrappers
such as `decorate(fn) { return function(...args) { return fn(...args) } }` and
`d(20, 22)`. The fixed-arity `relay(fn, xs)` path is resolved; that ABI follow-up
is not. The D2 named-function spread slice above is implemented separately
in PR #299 (pending review/merge); its checkboxes are ticked there.
