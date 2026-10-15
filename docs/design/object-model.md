# Crush object model: Kinds — one mechanism, many object semantics

Status: **draft for review** (vega, 2026-10-15). Answers CRUSH-251's "design note: value representation
of JS objects vs `Value::Map`" and widens it from JS to every walker language. Facts below are from
`origin/main` at `c864367`.

## The problem

Crush hosts many languages. Their object models genuinely disagree:

| | lookup / inheritance | receiver | construction | value or reference | fields | operators | interfaces |
|---|---|---|---|---|---|---|---|
| **JavaScript** | prototype chain, mutable at run time | dynamic `this` (set by the call site) | `new F(..)`, prototype from `F.prototype` | reference | open (add/delete any time) | fixed (`+` is ToPrimitive) | duck typing |
| **Python** | class MRO (C3), multiple inheritance | bound method (`obj.m` captures `self`) | `C(..)` → `__new__` + `__init__` | reference | open (`__dict__`), `__slots__` | dunder methods + reflected (`__radd__`) | duck typing, ABCs |
| **Rust** | none; `impl` blocks + traits, static dispatch (`dyn` for dynamic) | explicit `self` | struct literal, `T::new` by convention | **value** (move/copy/clone) | closed, fixed layout | traits (`Add`, `Display`, `PartialEq`) | nominal traits |
| **Go** | none; embedding promotes fields/methods | explicit receiver (value or pointer) | composite literal | **value** (pointer for sharing) | closed | none | **structural** (implicit satisfaction) |
| **C** | none | none | literal / init | **value** | closed, laid out | none | none |
| **Dart / Kotlin / Java** | class + mixins/interfaces, single inheritance | implicit `this` | `C(..)` | reference | closed | `operator +` | nominal |
| **Crush** | `struct` (fields only) | — | `P { .. }` | reference (today) | checked at parse time only | — | — |

**Today Crush has none of this.** CAST and the VMs have no class, prototype, vtable, method dispatch or
receiver binding.
- **Objects are field maps** (`Value::Map`). A Crush `struct` is a parse-time field check plus a `NEW_OBJ`
  whose name is dropped.
- **Methods are global names.** `obj.m(x)` compiles to a call to the **global** host capability `m` with `obj`
  as its first argument, dispatched by a flat string match (`scheduler.rs`, e.g. `"append" | "push"`). A
  function stored in a field can't be called as `o.f(x)`.
- **Each walker improvises:**
  - JS classes become object literals (`extends`/`constructor`/getters are loud), `new X` calls a function
    named `"new X"`, and `this` is never bound;
  - Python `class` is loud;
  - Rust `struct`/`impl`/`trait`/`enum` are loud;
  - Go, Dart, Zig and the boa JS lowering **silently drop** type declarations.
- **No higher-order-function library is shared.** The stdlib's `collections.sort_by` takes a field-name
  *string*, because host capabilities can't call back into a `Func`.

CRUSH-251 has delivered function values and closures (steps 1–4). Its remaining slice (prototype get/set,
`call_method`/`this`, `construct`/`new`) is written for JavaScript. Doing that, then Python classes, then Rust
traits, then Go interfaces, each as its own set of special cases in the VM and walkers, would repeat the
mistake the parity work keeps finding: every walker re-implements semantics, and a value loses its meaning
the moment it crosses into code from another language.

## Options considered

- **A. One fixed object model for everyone** (JS-like prototypes, or Python-like classes). This is what
  `docs/design/crushvm-rustpython.md` implies ("one VM, one object model, one GC; frontends provide syntax
  only"). It's lossy in both directions: Python's MRO and `__radd__` don't fit prototypes; Rust/Go/C value
  semantics don't fit any reference model; JS's mutable prototypes don't fit classes. Every mismatch becomes
  a silently-wrong program. **Rejected.**
- **B. Per-walker desugaring to maps and functions** (today, extended). Each walker encodes its language's
  rules in its own lowering. Works only in a closed world. Semantics get duplicated across walkers (and again
  in bozo, the AOT backends and the JIT), and an object passed between languages has no rules left.
  **Rejected as the general answer**, though it stays fine for purely static cases (below).
- **C. Run each language's real runtime** (CPython, V8) for objects. Ruled out by the owner's #204 decision:
  walked code never silently becomes host CPython or node (trust, determinism, capabilities).
- **D. Kinds: a small meta-object protocol in the VM, with each language's object model written as data and
  library code.** One VM, one value representation, one GC, as `crushvm-rustpython.md` wants, but with
  **parameterised** semantics instead of a single fixed one. **Recommended.**

## The concept: Kinds

A **Kind** is a first-class runtime value that says how a family of values *behaves*. Every object points at
its Kind. All object operations in CASM are a handful of **generic ops** that ask the object's Kind what to
do. Each language supplies its object model as a **kind library**: functions that build and wire Kinds the
way that language does (`py.type(name, bases, ns)`, `js.class(..)`, `rust.impl(..)`, `go.struct(..)`).

```
value ──► Record { kind, slots }          generic ops            Kind
                     │                     GET_MEMBER  ─┐        ├─ name, home language
                     └─────────────────────► SET_MEMBER  ├─ ask ──►├─ resolution order (list of Kinds)
                                             CALL_MEMBER │        ├─ members: methods, fields, accessors
                                             CONSTRUCT   │        ├─ hooks: missing / set / call / iterate /
                                             OP (+,==,<) │        │         op:+ / eq / hash / str / repr …
                                             ITERATE    ─┘        └─ flags: value|reference, open|sealed,
                                                                           receiver = this|bound|explicit
```

### 1. Values

- **`Record { kind: Rc<Kind>, slots }`**. Slots are laid out by a per-Kind *shape*, so field access can be an
  indexed load once the shape is known. An open-field language (JS, Python `__dict__`) gets an overflow map.
  FastVM's arena already has an unused `Object { lang, class_name, fields }`; this gives those slots meaning.
- **`Kind`** is itself a value (JS prototypes are objects; Python classes are objects), so a kind library can
  build and mutate Kinds at run time. Mutation bumps a version so caches stay correct.
- `Value::Map` stays as the plain dictionary. JS object literals become Records of the `js.Object` Kind, and
  Python dicts stay Maps.

### 2. Generic ops (the whole VM surface)

| op | meaning | what the Kind decides |
|---|---|---|
| `GET_MEMBER obj name` | read a field/method/property | lookup along the resolution order; accessor/descriptor hooks; `missing` hook (`__getattr__`, JS `undefined`, Rust compile-time error) |
| `SET_MEMBER obj name v` | write | open vs sealed; setter hooks; frozen |
| `CALL_MEMBER obj name args` | method call | **receiver policy**: JS binds `this` to `obj`; Python binds `self`; Rust/Go pass the receiver explicitly (by value or reference per the method) |
| `CONSTRUCT kind args` | create an instance | JS `new` (allocate with proto + call constructor); Python `__new__`/`__init__`; Rust/Go/C literal construction |
| `OP op a b` | `+ - * == < …` on objects | left operand's Kind hook, then the language's reflected rule (Python `__radd__`), then primitives |
| `ITERATE obj` / `NEXT it` | `for x in obj` | iterator hook (`__iter__`, `[Symbol.iterator]`, `IntoIterator`) |
| `HAS_MEMBER` / `DEL_MEMBER` | `in` / `delete` / `hasattr` | as above |

**Bound methods** are a `Func` value with a receiver slot, created by `GET_MEMBER` when the Kind's policy is
`bound` (Python). JS's policy returns the plain function, so `const g = o.m; g()` loses `this`, as in JS.
The same CASM produces each language's behaviour.

### 3. Protocols: what higher-order functions need

- **Callable:** `Func`, a bound method, or any object whose Kind has a `call` hook (`__call__`).
- **Iterable:** anything `ITERATE` accepts.
- **Comparable / hashable / printable:** the `eq` / `hash` / `str` / `repr` hooks, with defaults per language.

On top of these, **one HOF library written once, in Crush** (a prelude, compiled like user code, so it can call
`Func`s, which host capabilities can't): `map, filter, reduce, sort_by(key), any, all, zip, enumerate,
flat_map …`. Each language's API is a thin wrapper that keeps that language's contract:
- JS `Array.prototype.map` is eager and passes `(x, i, arr)`;
- Python `map` is a lazy iterator, and `sorted(key=)` is stable;
- Rust iterator adapters are lazy until `.collect()`.

Laziness lives in the wrapper; the core is shared. This also unblocks #175 `parallel_map`.

### 4. Kind libraries, one per language

| library | builds | notes |
|---|---|---|
| `lang/crush` | `struct`, and later `impl`/`trait` | Crush itself is the first consumer: native structs get a real Kind, which fixes `Pattern::Struct` ignoring the struct name |
| `lang/js` | prototypes, `class`/`extends`/`super`, `new`, getters/setters | Kinds are mutable objects; the resolution order is the prototype chain |
| `lang/py` | `type(name, bases, ns)` with **C3 MRO**, dunders, bound methods, properties | descriptors/metaclasses: a supported subset, the rest loud |
| `lang/rust` | `struct` (value), `impl`, `trait` (static dispatch; `dyn` = a vtable Kind), `enum` (tagged Kinds) | value semantics: copy/move/clone decided at compile time |
| `lang/go` | `struct` (value), methods, embedding (promoted members), `interface` (structural check) | interface satisfaction checked on assignment |
| `lang/c` | value structs, no methods | layout-only Kind |
| `lang/dart` (+ Java/Kotlin later) | classes, mixins (linearised like Python) | — |

Most of each library is **Crush code**, with a few native helpers (C3 linearisation, structural interface
check). That puts language semantics in one reviewable place per language, out of the VM, the walkers and the
AOT backends. The walkers then lower syntax to kind-library calls plus generic ops, and **stop encoding
semantics themselves**.

### 5. Value semantics

The Kind's `value` flag drives the compiler (statically known for Rust, Go and C):
- assignment, argument passing and `return` copy, or move;
- `.clone()` deep-copies;
- reference Kinds share.

This fixes the bug class the #204 review found: `let mut b = a.clone(); b.push(4)` currently mutates `a`,
because Crush arrays are reference types and `clone` is a no-op.

### 6. Static fast paths

When a walker knows the Kind statically (Rust, Go, C, sealed classes, Crush structs), the compiler emits **direct**
ops: `GET_SLOT i` and direct `CALL` to the resolved method. No dispatch, and AOT-compilable on day one. Dynamic
generic ops get **inline caches** keyed on (Kind version, shape). On AOT, dynamic member ops start loud
(exact-or-loud), like closures today.

### 7. Crossing languages

**An object keeps its home semantics.** A Python object used from JS code still looks up attributes by Python's
MRO, binds `self`, and adds via `__add__`, because the ops ask *its* Kind. Primitive values (numbers, strings,
lists) convert at the boundary by explicit, documented rules (for example, JS number to Python int when integral).
This is what makes crush-catalyst's "physics in JS, inference in Python, one process" credible without a
lowest-common-denominator object model.

### 8. Memory

Kinds, methods and instances form cycles (a class refers to methods that refer to the class). Today's `Rc`
values leak them, as CRUSH-251 §9 already accepts for closures. Kinds make cycles normal, so this design depends
on CRUSH-121's direction (arena plus a tracing collector with a correct root set). Interim: Kinds are immortal
per program; instance cycles leak until CRUSH-121.

## How this changes existing plans

- **CRUSH-251.** Its object-model half (prototype get/set, `call_method`/`this`, `construct`) becomes the
  generic ops above, with JS prototypes as one kind library instead of the VM's built-in model. Closures (done)
  are unchanged.
- **CRUSH-250** (the boa JS frontend) emits kind-library calls plus generic ops. **bozo BOZO-M7-1/2's** "classes
  desugared to prototypes" and runtime library land here.
- **Python.** `docs/design/python-lowering-coverage.md` suggested leaving MRO/`super()` to a CPython fallback,
  which the #204 decision rules out. With Kinds, C3 is a small native helper and `super()` is a lookup starting
  after the current class in the MRO.
- **Rust (#204, xps-5).** Structs, impls, traits and the value-semantics bug class get a home.
- **`crushvm-rustpython.md`'s principle is kept and sharpened:** one VM, one representation, one GC, and frontends
  supply syntax. Language *semantics* move into kind libraries, not walkers.

## Phasing (proposed tickets)

| phase | delivers | proves |
|---|---|---|
| **K1** | `Record` + `Kind` values; `GET/SET/CALL_MEMBER`, `CONSTRUCT`; receiver policies; **`o.m(x)` stops compiling to a global capability** (methods resolve on the receiver; capability namespaces like `net.fetch` keep their syntax); Crush-native `struct` + methods as the first kind library | one language end to end |
| **K2** | the protocols (callable/iterable/eq/hash/str); `OP` dispatch; the HOF prelude in Crush; a VM re-entrancy API so host capabilities *can* call `Func`s | HOFs across languages |
| **K3** | kind libraries: Python classes (C3, dunders, bound methods, `super`), JS classes/prototypes/`new`/`this` (with CRUSH-250), Rust structs/impl/traits/enums with value semantics, Go structs/methods/embedding/interfaces | each against its parity harness (cpython_corpus, node_corpus, rustc_corpus, a Go corpus) |
| **K4** | static fast paths (`GET_SLOT`, direct calls), inline caches, AOT for static Kinds | performance + AOT exact-or-loud |
| **K5** | cross-language tests: a Python object used from JS in one program, and the reverse | the catalytic thesis |

Each phase lands on the scheduler VM and portable VM first; FastVM/JIT/AOT are loud until they implement it.
The bar is the parity spike's: VM exact, AOT exact-or-loud, never silently wrong.

## Decisions for the owner

1. **Adopt Kinds (option D)** as the object model for Crush and every walker, instead of the JS-specific
   remainder of CRUSH-251 plus per-language special cases.
2. **Fidelity per language:** which dynamic corners are supported and which are loud (Python
   metaclasses/descriptors beyond `property`; JS `Proxy`/`Reflect`/`Object.defineProperty`; Rust generics
   beyond monomorphic use). Proposal: the subset each parity corpus needs, everything else loud.
3. **Value semantics:** enforce move (a use after move is a loud error) or copy-on-assign for Rust? Proposal:
   copy for `Copy` types, move with a loud use-after-move for the rest, as rustc does.
4. **Crush-native surface:** does Crush itself grow `impl`/`trait` (or a `kind` declaration) on top of the same
   mechanism, or keep `struct` only for now?
5. **GC:** accept leaking instance cycles until CRUSH-121, or make CRUSH-121's root-set fix a prerequisite of K3?

## Bugs found during this survey (independent of this design)

**Reproduced on `origin/main` `c864367`, silently wrong:**
- **Crush `match` on a struct pattern ignores the struct's name** (`compiler.rs` `Pattern::Struct`).
  `struct P { x: int } struct Q { x: int }`, then `match Q { x: 5 } { P { x: a } => 1, Q { x: b } => 2 }`
  returns **1** (expected 2): the first arm with matching fields wins.
- **The Python walker discards call keyword arguments** (`lower_call(.., _keywords, ..)`).
  `print('a', end=''); print('b')` prints `a⏎b` (CPython `ab`), and `print('c', 'd', sep='-')` prints `c d`
  (CPython `c-d`).

**From a code read, not yet reproduced:**
- **Go, Dart, Zig walkers and the boa JS lowering:** type declarations (`type … struct`, `interface`, classes)
  are silently dropped (`_ => Ok(None)` / `NullLiteral`). Same pattern as JS CRUSH-zorro-23; each needs a
  "make it loud" pass.
- **Rust walker:** non-identifier closure parameters silently become `_param`.
