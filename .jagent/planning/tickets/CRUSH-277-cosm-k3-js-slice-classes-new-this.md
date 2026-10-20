# CRUSH-277 — COSM K3 (JS slice): JS classes, `new` and `this` on COSM

| Field | Value |
|-------|-------|
| **ID** | CRUSH-277 |
| **Priority** | P1 |
| **Status** | In progress — S1 done (`agent/muse/CRUSH-277`), S2 done (`agent/muse/CRUSH-277-s2`), S3 done (`agent/muse/CRUSH-277-s3`); S4 next |
| **Phase** | COSM K3 — JS walker slice |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-266 (exports, PR #283), CRUSH-zorro-43 (COSM VM ops, done), CRUSH-zorro-44 (receiver method calls; partial — see D4), CRUSH-276 (blocker queue; this ticket scopes its item 1) |
| **Estimated effort** | L total; S1 M, S2 S–M, S3 M, S4 M, S5 M |
| **Filed by** | muse — 2026-10-20, scoped against `origin/main` at PR #283 |

## Problem

JS `class`, `new` and `this` are refused by the boa lowering
(`crates/crush-lang-js/src/lower/expr.rs`: "`this` (waits for the COSM
object model)", "`new` …", "a class …"; `lower/stmt.rs`: "a class (waits
for the COSM object model)"), because no source-language walker emits COSM
yet. The design doc (`docs/design/object-model.md`, phasing table) assigns
this to **K3**, and owner decision 2 keeps everything outside the parity
corpus's needs loud until a strategy is added deliberately.

The corpus pressure is measured (CRUSH-276): in maya-engine (37 ESM files,
4,643 LOC) classes are the first blocker for 19 files on the SWC lowering.
Census of the JS class surface maya actually uses:

| feature | uses |
|---|---|
| `class` declarations | 34 |
| files with a `constructor` | 25 |
| `this.` references | 1,855 |
| `extends` | 8 (all named local bases: `Component`, `Scene`, `Error`) |
| `super(...)` / `super.m(...)` | 5+2 / 9 |
| `get` / `set` accessors | 11 |
| class fields (`x = …` in body) | ~30 |
| `static` methods, `#private` | 0 / 0 |
| `new` of own classes | ~20 (`Body`×7, `FrameStore`, `SpatialHash`, …) |
| `new` of host/builtins | dominant (`THREE.*`, `Set`, `Map`, `Error`, typed arrays) |

## What already exists (do not rebuild)

- **Schema**: `crush-cosm` has every JS strategy — `Resolution::PrototypeChain`,
  `Receiver::DynamicThis`, `Construct::JsNew`, `Member::Accessor`,
  `Member::Static`, open-dict storage; `resolution_order` already walks a
  prototype chain (`crates/crush-cosm/src/mro.rs:237`, shared with `Single`).
- **CASM/VM**: `CONSTRUCT`, `CALL_MEMBER`, `GET/SET_MEMBER`, `IS_KIND` ops
  exist; scheduler + portable VMs implement them (`crates/crush-vm/src/cosm_rt.rs`).
  `kind_constructor` already resolves `JsNew` to an initialiser function
  (`crates/casm/src/lib.rs`), and `call_member` already binds `DynamicThis`
  receivers (recv as leading arg; a detached method gets `this = Null`,
  matching JS `undefined` under the VM's null spelling).
- **Compiler**: `C(…)` on a constructor-Kind emits `construct`
  (`compiler.rs:2194`); a dotted call naming a receiver method emits
  `call_member` instead of a capability (`compiler.rs:2314`);
  `constructed_kind` / `receiver_methods` are driven by the program's
  `CosmModule`.

## The gaps (the actual work)

1. **Transport.** `crush_cast::Program` has no `cosm` field; the frontend
   derives COSM only from Crush `struct`/`impl`/`trait` statements
   (`crush_frontend::cosm_lower`). A JS walker has nowhere to put a
   `KindDecl`.
2. **The shared gate refuses JS strategies.**
   `casm::kind_unsupported_strategies_on(…, CosmSupport::Kinds)` refuses
   `prototype_chain` ("their walkers, K3") and accessor members, so a JS
   `KindDecl` is rejected at compile time even if emitted. FastVM/JIT/AOT
   refuse COSM wholesale (`cosm_gate.rs`) — that stays.
3. **No CAST spelling for `new` vs a plain call, nor for loading a Kind as
   a value.** `constructed_kind` triggers on `Call { function: "C" }`,
   which cannot distinguish `new C()` from `C()`. There is also no op that
   materialises `Value::Kind`, so `Foo.staticM()`, passing a class as an
   argument, and `x instanceof Foo`'s right-hand side have no path
   (`IS_KIND` takes a static name, which covers `instanceof` only when the
   RHS is a bare class name — the common case).
4. **`super`.** `find_on_kind` always starts lookup at the receiver's own
   Kind; there is no "start after kind K" lookup for `super.m()`, and a
   subclass with no declared constructor gets no default
   forward-to-`super` constructor.
5. **Accessors at run time.** `Member::Accessor` is schema-complete but
   `cosm_rt::find_on_kind` refuses it and `get_member`/`set_member` never
   invoke accessor functions. Maya uses 11 get/set pairs.
6. **Constructor return override.** JS: a constructor returning an object
   yields that object. The scheduler's `CONSTRUCT` sequence `POP`s the
   initialiser's return value. Divergence to resolve deliberately (D6).

## Design decisions

- **D1 — Transport: extend CAST, don't add CAST statements.**
  Add `cosm: Option<crush_cosm::CosmModule>` to `crush_cast::Program`
  (serde-default, omitted when `None`, exactly like `casm::Program::cosm`
  from CRUSH-zorro-42, so old CAST serialises identically). Note the
  layering comment in `crates/crush-cosm/Cargo.toml`: `crush-cosm` stays a
  leaf (serde-only); `crush-cast → crush-cosm` is acyclic. The frontend
  merges a walker-supplied module with `cosm_lower`'s output in
  `compile_cast_with_cosm`, rejecting name collisions, and runs the
  existing `validate` + `refuse_unimplemented_strategies`.
  *Rejected alternative*: CAST `ClassDef` statements re-derived by
  `cosm_lower` — duplicates the COSM schema in CAST and forces JS through
  Crush-surface checks (`self` rules) that don't apply.
- **D2 — `this` is an implicit first parameter, no new CAST node.**
  The walker lowers each method/constructor to a CAST function whose first
  parameter is named `this`; `this` expressions lower to `Var("this")`.
  This matches the VM's existing convention (receiver passed as leading
  argument; `DynamicThis` detached calls capture `Null`). Free-variable /
  scope analysis must treat `this` as bound inside methods and refuse it
  (loud, as today) at top level and inside arrow functions that would
  capture an outer `this` — arrow-function `this` capture is out of scope
  until it gets its own lowering (JS arrow `this` is lexical; silently
  binding the wrong one is the failure mode to avoid).
- **D3 — `new C(args)` gets a CAST node.**
  Add `Expression::Construct { kind: String, args }` (mirrors the CASM op;
  the compiler emits `construct`, checking `kind` against the program's
  COSM module — generalising `constructed_kind` rather than overloading
  `Call`). `new` on anything that isn't a walker-declared Kind name
  (member expressions `new THREE.Vector3()`, builtins `new Set()`) stays
  on the existing host/loud path — that is CRUSH-110/builtin territory,
  not this ticket.
- **D4 — Method calls ride CRUSH-zorro-44, conservatively.**
  `this.m(...)` and calls on receivers the walker can name (a variable the
  module itself bound via `new C()`) may emit the existing dotted-call
  CAST that the compiler already routes to `call_member` when `m` is a
  receiver method. The general problem — a method name colliding with a
  builtin (`xs.push` on an array vs a user class's `push`), and
  `CALL_MEMBER` on a non-Record erroring instead of falling back — is
  zorro-44's dispatch rule and must land there or here-with-its-tests;
  this slice must not flip builtin method calls for non-Record receivers.
- **D5 — `extends` and `super`.**
  `extends Name` (identifier, local or imported) → `parents: [KindRef]`.
  Expression superclasses (`extends mixin()`, `extends THREE.Object3D`)
  stay loud. `super(args)` in a constructor → `CONSTRUCT` of the parent
  Kind on the same instance (needs a `CONSTRUCT`-on-existing-instance
  path or a parent-initialiser call in `cosm_rt` — implementer's choice,
  but the instance identity must be shared). A subclass with no
  constructor synthesises the JS default (forward all args to `super`).
  `super.m(...)` → lookup starting after the current Kind in the
  receiver's resolution order (new `cosm_rt` entry point; sender Kind is
  a walker-known constant). Order: `super()` first (S3), `super.m()`
  in S5 — maya uses both (7 vs 9).
- **D6 — Constructor return override: owner decision needed.**
  Either honour it (scheduler/portable `CONSTRUCT` keeps a Record-typed
  initialiser result) or declare it loud/unsupported in the walker when a
  constructor `return`s a value. Recommendation: honour it — it's three
  lines next to the existing `POP` and silently dropping it is the
  CRUSH-zorro-23 failure mode. Flagged here because it changes shared
  `CONSTRUCT` semantics, not just JS.
- **D7 — Class fields initialise in the constructor.**
  Field definitions (with/without initialisers) lower to `this.f = …`
  statements injected at the JS-specified point: after `super()` returns
  in a derived class, at construction start in a base class; a
  synthesised constructor when none is declared. Static fields/methods →
  `Member::Static` on the Kind (Kind-value member access already works in
  `cosm_rt` once D8 lands).
- **D8 — Loading a Kind as a value.**
  Add a `load_kind` CASM op + compiler rule: `Var(name)` where `name` is
  a Kind in the program's COSM module and not shadowed by a local emits
  `load_kind`. Scheduler/portable implement it (`Value::Kind` from the
  loaded kind table); FastVM/JIT/AOT refuse via the existing COSM gates.
  This unblocks statics, classes-as-arguments, and `instanceof` by name
  (existing `is_kind`).
- **D9 — GC prerequisite does not block this slice.**
  Design decision 5 names CRUSH-121 as a K3 prerequisite. That GC is
  FastVM's arena collector; FastVM refuses COSM programs today.
  Scheduler/portable values are `Rc`-based and Kind/instance cycles
  already leak by accepted interim (design doc §8). So: S1–S5 proceed on
  scheduler + portable; **enabling COSM on FastVM** stays blocked on
  CRUSH-121 step 1. Asking the owner to confirm this reading at pick-up.
- **D10 — Boa only.** The SWC lowering keeps its current class behaviour
  untouched by this ticket (it's the retiring path; CRUSH-250 step 6
  flips `.js` to boa). Its silent method-object-literal class lowering
  becoming loud is CRUSH-zorro-23 hygiene, filed separately if picked up.

## Slices

- **S1 — Transport + walker emission (boa).** D1 merge/validate;
  `lower/stmt.rs` class declarations and `lower/expr.rs` class
  expressions emit `KindDecl`s (`open_dict`, `reference`,
  `prototype_chain`, `dynamic_this`, `js_new(ctor)`, named `extends`
  parents, methods/statics as members, constructors as lifted functions
  with D2's `this` parameter) into `Program::cosm`; class name bound in
  scope (already registered, `lower/scope.rs`). Nothing runs yet — the
  gate still refuses `prototype_chain`, loudly. *Tests*: CAST+COSM shape
  tests beside `boa_backend_test.rs`; CAST serde round-trip unchanged for
  cosm-less programs.
- **S2 — VM: prototype chains.** Lift `prototype_chain` from
  `kind_unsupported_strategies_on` for `CosmSupport::Kinds`;
  `crush-cosm::validate` checks for the strategy (acyclic parents, ctor
  is a declared function); scheduler/portable tests through
  `crush-vm/tests/kind_table.rs` + `records.rs` patterns: inherited
  method, instance field shadowing a prototype member, overridden
  method, detached method (`this` unbound), `instanceof` via `is_kind`.
  *FastVM/JIT/AOT still refuse the whole table — assert that too.*
- **S3 — `new`, `this`, method calls end to end (boa).** D3 `Construct`
  CAST node + compiler; `this` expressions; D8 `load_kind`; D5
  `super()` + synthesised forwarding constructors; `this.m()` calls.
  *Tests*: node_corpus class cases (below) run green on scheduler;
  CRUSH-266-style end-to-end test (`class Counter { … }`, mutate via
  `this`, read back) prints the node-identical result.
- **S4 — Accessors.** `cosm_rt` `get_member`/`set_member` invoke
  `Member::Accessor` get/set with the receiver bound per the Kind's
  receiver strategy; lift the accessor refusal for `CosmSupport::Kinds`.
  *Tests*: get-only, set-only, inherited accessor, accessor on the
  prototype vs instance field shadowing (JS order: own field wins —
  it's checked first already).
- **S5 — `super.m()`, fields, statics, corpus + maya pass.** D5
  start-after lookup; D7 field injection; statics through `load_kind`;
  node_corpus extended; maya-engine walk re-measured and recorded in
  CRUSH-276 (classes should stop being the first blocker; the walk then
  hits CRUSH-276's next items, which is the expected outcome, not a
  regression).

## Progress

- **S1 — done** (branch `agent/muse/CRUSH-277`). `crush_cast::Program::cosm`
  added (serde-default/`None`-omitted, so old CAST is byte-identical;
  `crush-cast → crush-cosm` dependency, acyclic as the crate's layering
  comment requires); `compile_cast_with_cosm` merges the walker module
  with `cosm_lower`'s output via `cosm_lower::merge_walker_module`
  (collision = error, merged module validated). Boa lowering
  (`lower/class.rs`): class declarations, exported classes and named
  class expressions emit KindDecls with the D2 `this`-parameter
  convention; accessors merge into one member; statics are
  `Member::Static`; derived-without-constructor, expression superclasses,
  class fields (S5), private members, static blocks, computed names,
  async/generator methods and `this` outside a direct method body all
  stay loud. Tests: `crush-cast/tests/cosm_field_tests.rs` (3),
  `crush-frontend/tests/walker_cosm_tests.rs` (5, incl. the
  prototype-chain refusal), 11 class tests in `boa_backend_test.rs`;
  workspace `--all-targets` check and CI-style clippy clean.
  Maya sweep (boa, after S1): no file is blocked by class syntax itself;
  `engine/core/Time.js` walks with kind `Time` emitted. First blockers
  are now the rest of CRUSH-276's queue — `export function` (PR #283),
  `new` (S3), optional chaining, `.reduce()` (zorro-20), `Object.*`
  (zorro-16), re-exports — plus one discovery below.
- **Discovered during S1**: method/constructor *default parameters*
  (`constructor(x = 1)`) are refused, because methods lower with the
  function-expression restrictions (`declared: false`) — maya hits this
  in `GameObject`, `Scene`, `Physics`, `Engine`, … This is zorro-18's
  territory extended to methods; `CALL_MEMBER`'s exact-arity check means
  it needs the hidden-flag ABI or a dispatch rule, so it belongs with
  S3/zorro-44, not S1. Also `Protocol.js` shows derived classes without
  constructors are common — S3's default-`super` forwarding matters.

- **S2 — done** (branch `agent/muse/CRUSH-277-s2`, stacked on S1).
  `prototype_chain` removed from `casm::kind_unsupported_strategies_on`
  at `CosmSupport::Kinds` (embedding_promotion stays refused); the
  refusal-message/docs updated in casm and `crush-vm/src/cosm_gate.rs`.
  No VM code changes were needed: `resolution_order` already linearised
  prototype chains, `find_on_kind` walks the order, `DynamicThis`
  binding and `js_new` construction were already wired — the gate was
  the whole blocker. New runtime evidence:
  `crush-vm/src/tests/prototype_chain.rs` (7 tests × both VMs) —
  js_new construction with `this` state, method inheritance + override,
  `instanceof` along the chain, instance fields shadowing prototype
  members, detached methods as plain functions (and the loud null-`this`
  failure when one reads `this`), prototype cycles refused at load.
  Existing refusal tests retargeted at still-refused strategies
  (frontend `cosm_lowering`, vm `kind_table`, sdk `cosm_kind_table`);
  the S1 gate tests now assert compilation. A follow-up fix on the S1
  branch retargeted two `crush-lang-sdk` polyglot refusal tests that
  S1's class lowering had un-louded. Suites: casm + crush-vm +
  crush-frontend + crush-lang-js + crush-lang-sdk = 1,418 tests green;
  workspace `--all-targets` and CI-style clippy clean.

- **S3 — done** (branch `agent/muse/CRUSH-277-s3`, stacked on S2).
  CAST gained `Expression::Construct { kind, args }` (compiler emits
  `construct`, erroring on an undeclared kind); casm gained `load_kind`
  (CVM1 `0xCB`), implemented on the scheduler and portable VMs and
  refused on FastVM (whose default arm silently NOPs unknown bytes);
  a class name as a value compiles to it (D8). Boa: `new C()` over
  declared classes only (host constructors and member callees stay
  loud), bare `C()` refused, `super(...)` calls the parent's
  constructor on the same instance, derived-without-constructor
  synthesises forwarding constructors, `Util.m()` statics and
  `this.m()`/`c.m()` (bindings bound with `new C()`) lower to member
  calls (D4 conservative). Two bugs found by the end-to-end tests and
  fixed here: statics took the implicit `this` parameter although the
  VM calls statics with no receiver (now plain parameters, `this`
  bound to the class locally); and `const C = class {}` loaded its own
  uninitialised binding (the compiler counts the binding declared
  during the initialiser — the walker now goes through a hidden temp;
  the compiler's declared-wins ordering is unchanged). The casm
  version-stamp bug the round trip exposed is CRUSH-278. Evidence:
  boa e2e runs through CASM JSON → blob → scheduler VM print
  `42`/`7`/`9` (construction + `this` state, `super()` + inherited
  method, forwarding ctor, statics, class-expression `new`), frontend
  `Construct`/`load_kind` compile tests, VM `load_kind` + static call
  tests on both VMs. Suites: workspace `--all-targets` green except
  the pre-existing `crush-frontend` `cast_compile` bench failure
  (identical on the S2 base; meta-serialisation drift, unrelated) —
  CI-style clippy clean.
  Maya sweep (boa, after S3): `engine/FixturePlayer.js` and
  `engine/core/Time.js` walk; S3's own features are the first blocker
  nowhere. Remaining first-blockers are the known queue: `export
  function` (PR #283), host `new` (`Map`, three.js — CRUSH-110/host),
  method default parameters (now the top walker item, 8 files —
  zorro-18-for-methods, as flagged in S1), `?.`, `.reduce()`,
  `Object.assign`, zorro-44 member calls on parameters.

## Success criteria

- [ ] A JS class with constructor, methods, `this` state, `extends` +
      `super()`, a getter/setter pair, a class field and a static —
      compiled from boa through CAST+COSM → CASM → scheduler VM — runs
      with node-identical observable output, via `node_corpus` harness
      cases added in the same PRs as each slice.
- [ ] Every construct outside the slice stays a loud refusal naming the
      construct (expression superclasses, `#private`, static blocks,
      prototype mutation `F.prototype.m = …`, `Proxy`/`Reflect`,
      arrow-function `this`, `new` of non-Kind callees).
- [ ] No behaviour change for cosm-less programs (CAST/CASM serde
      compatibility asserted by round-trip tests), and the full
      `crush-lang-js`, `crush-frontend`, `crush-vm`, `casm` suites pass.
- [ ] maya-engine boa walk re-measured at S5 and recorded in CRUSH-276.

## Non-goals

- Running maya-engine against three.js (needs CRUSH-110 module
  resolution and a three/DOM/WebGL host capability).
- JS object literals as `js.Object` Records, prototype mutation,
  `Object.create`/`defineProperty`, `Proxy` — Maps stay Maps; a later
  slice if the corpus demands it (design decision 2).
- Python/Rust/Go K3 slices, K2 protocol hooks, K4 fast paths,
  FastVM/JIT/AOT COSM support (blocked on CRUSH-121; D9).

## Risks / open questions

- D6 (constructor return override) and D9 (GC prerequisite reading) want
  owner confirmation at pick-up; neither blocks S1–S2.
- `receiver_methods` is a bare-name set across all Kinds; a JS class
  method named like a Crush builtin is exactly zorro-44's collision
  case — S3's conservatism (D4) is load-bearing, don't "simplify" it.
- Cross-module class identity (`import { Component }` then
  `extends Component`) is name-based until CRUSH-110 gives modules real
  bindings; KindRefs may collide across modules. Acceptable for the
  corpus slice; note it in code where the merge happens (D1).
