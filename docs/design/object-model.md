# COSM — the Crush Object Semantic Model

*Kinds: one mechanism, many languages' object semantics. Objects are lowered, like code.*

Status: **design; owner decisions taken 2026-10-15** (vega). Answers CRUSH-251's "design note: value
representation of JS objects vs `Value::Map`" and widens it from JS to every walker language. Facts below are
from `origin/main` at `c864367`.

**Naming:** **COSM** (Crush Object Semantic Model) is the lowered, declarative object model, the object-side
sibling of CAST (code) and CASM (instructions). A **Kind** is a COSM declaration instantiated at run time.

**COSM vs CAISON.** They're different layers. **CAISON** (the `caison` crate, formerly CSON) is a *data
notation*: how a document or config is written down (`MILESTONES.caison`), parsed into plain values. **COSM** is
a *semantic model*: what a type is and how its instances behave at run time. They meet in two places:
- COSM declarations need a serialized form. Like CAST's JSON, COSM can be serialized as **CAISON**, dogfooding
  the house format.
- A CAISON document's `schema:` can be expressed as a COSM `KindDecl`, so parsing yields typed, validated
  objects instead of untyped maps.

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

## COSM: objects are lowered, like code

Every walker already lowers *code* to one shared form, CAST, so the compiler, VMs and AOT backends never see
JavaScript or Python, only CAST. COSM does the same for *objects*. A walker lowers each language's object
declarations (JS `class`, Python `class`, Rust `struct`/`impl`/`trait`/`enum`, Go `type … struct`/`interface`,
C `struct`, Dart class/mixin, Crush `struct`/`impl`/`trait`) to **COSM declarations**: plain, serializable
data, like CAST nodes.

```
 source ──walker──►  CAST   (functions, statements, expressions)  ──compiler──►  CASM ──► VM / AOT
                ╲
                 ►  COSM   (KindDecl / InterfaceDecl: what each type is and how it behaves) ──► Kinds at load time
```

**What differs between languages is captured as a small, closed set of named strategies in the
declaration**, not as per-language code. Each strategy is implemented **once** in the VM (and, where static,
in AOT) and shared by every language that uses it. Python and Dart both use the C3 linearisation; Rust, Go and
C share the value-semantics strategies.

```text
KindDecl {
  name, home_lang, span                          // for diagnostics, like CAST meta
  storage:     sealed { fields: [name: type, default?] } | open_dict | open_dict_with_slots
  semantics:   reference | value(copy | move | clone_explicit)
  parents:     [KindRef]
  resolution:  none | single | c3 | prototype_chain | embedding_promotion
  receiver:    dynamic_this | bound_self | bound_cls | explicit_value | explicit_ref | explicit_ref_mut
  construct:   literal | ctor(fn) | js_new(ctor_fn) | py_new_init(new_fn?, init_fn)
  members:     { name: method(fn, receiver?) | field | accessor(get?, set?) | static(value | fn) }   // a member may override the Kind's receiver
  hooks:       { missing, set, call, iter, op:<op>, op_reflected:<op>, eq, hash, cmp, str, repr }
  conforms:    [ InterfaceRef + check: nominal | structural ]
  equality:    identity | structural          // default when no `eq` hook
}
InterfaceDecl { name, home_lang, members: [signature], satisfaction: nominal | structural }
```

Why lower objects instead of running per-language libraries:
- **One semantics per strategy, not per walker.** The JS walker doesn't implement prototype lookup; it declares
  `resolution: prototype_chain`. bozo's frontend, the generic walker and AOT get identical behaviour for free.
- **Inspectable and checkable, like CAST.** COSM is validated at compile time:
  - an unlinearisable MRO is a compile error, as Python's `TypeError` is;
  - a missing trait method or a Go interface that isn't satisfied are reported with source spans;
  - a struct-pattern match tests the Kind (fixes CRUSH-zorro-37 properly).
  Tools (LSP, polydex, docs) read COSM directly.
- **Static where the language is static.** A sealed, value-semantics `KindDecl` (Rust, Go, C, Crush structs) has
  a fixed layout known at compile time, so the compiler emits `GET_SLOT i` and direct calls and AOT compiles it
  without any dynamic dispatch.
- **Dynamic where the language is dynamic, with the same schema.** JS prototype mutation and Python's runtime
  `type(name, bases, ns)` or class attribute assignment build or patch Kinds *at run time* in this same shape
  (a Kind is a live COSM declaration with a version number), so static and dynamic paths can't drift apart.

## The concept: Kinds

A **Kind** is a first-class runtime value that says how a family of values *behaves*. Every object points at
its Kind. All object operations in CASM are a handful of **generic ops** that ask the object's Kind what to
do. Kinds are instantiated from the program's COSM declarations when it loads, and built or patched at run
time only where the language itself is dynamic (JS prototypes, Python `type()`).

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

### 4. The strategy catalogue: what languages actually differ in

| dimension | strategies | used by |
|---|---|---|
| member lookup (`resolution`) | `none` · `single` · `c3` · `prototype_chain` · `embedding_promotion` | Rust/C/Go-methods · Java/Kotlin · Python/Dart · JS · Go embedding |
| receiver binding | `dynamic_this` · `bound_self` · `bound_cls` · `explicit_value` · `explicit_ref` · `explicit_ref_mut` | JS · Python · Python `@classmethod` · Rust `self`/Go value receivers · Rust `&self` · Rust `&mut self`/Go pointer receivers |
| construction | `literal` · `ctor` · `js_new` · `py_new_init` | Rust/Go/C/Crush · Dart/Java · JS · Python |
| storage | `sealed` · `open_dict` · `open_dict_with_slots` | Rust/Go/C/Crush/Dart · JS/Python · Python `__slots__` |
| value semantics | `reference` · `value(copy)` · `value(move)` · `clone_explicit` | JS/Python/Dart · C/Go/Rust `Copy` · Rust · Rust `Clone` |
| operators | `hooks` (+ `op_reflected`) · none | Python dunders, Rust trait impls, Dart `operator` · JS/Go/C |
| interfaces | `nominal` · `structural` · duck (no decl) | Rust traits/Java · Go · JS/Python |
| equality default | `identity` · `structural` | reference languages · Rust `PartialEq` derive/Go/C structs |

A new language is supported by choosing from this catalogue in its walker, not by writing a new runtime.
Something that fits no strategy (JS `Proxy`, Python metaclasses beyond `type`) is **loud** until a strategy is
added deliberately. That keeps the catalogue closed and reviewed.

**What's left of "kind libraries"** is small: native helpers for the strategies (the C3 linearisation, the
structural-interface check, the `js_new`/`py_new_init` sequences), and per-language default hooks (how Python
`repr`s an instance, JS's default `toString`). They live once, beside the VM, not in each walker.

### 4b. Worked examples

#### A Rust struct with impls and a trait

```rust
#[derive(Clone, PartialEq)]
struct Point { x: i64, y: i64 }

impl Point {
    fn new(x: i64, y: i64) -> Self { Point { x, y } }
    fn norm2(&self) -> i64 { self.x * self.x + self.y * self.y }
    fn shift(&mut self, d: i64) { self.x += d; }
}
impl std::ops::Add for Point { type Output = Point; fn add(self, o: Point) -> Point { /* … */ } }
trait Shape { fn area(&self) -> f64; }
impl Shape for Point { fn area(&self) -> f64 { 0.0 } }
```

The walker collects **every** `impl` block for `Point` in the file (they can appear anywhere) into one declaration:

```text
KindDecl Point  (home: rust)
  storage:    sealed { x: Int, y: Int }              slot 0 = x, slot 1 = y
  semantics:  value(move) + clone hook               not Copy, so assignment moves; derive(Clone) = deep copy
  resolution: none                                   no inheritance
  construct:  literal                                Point { x, y }
  members:
    new    : static(fn Point::new)
    norm2  : method(fn, receiver: explicit_ref)       &self
    shift  : method(fn, receiver: explicit_ref_mut)   &mut self
  hooks:      op:+ → <Point as Add>::add ;  eq → derived structural
  conforms:   Shape (nominal) → members Shape::area
InterfaceDecl Shape { area(&self) -> f64 ; satisfaction: nominal }
```

| Rust code | what happens |
|---|---|
| `let q = p;` | **move**: the compiler marks `p` moved; a later use of `p` is a compile-time error, as with rustc |
| `let q = p.clone();` | deep copy via the clone hook (today `clone` is a no-op, so mutating `q` mutates `p`) |
| `p.shift(3)` | the receiver is passed by reference, so the change is visible through `p`; a `self`-by-value method receives a moved or copied value |
| `a + b` | `OP +` → the `Add` impl |
| `p.area()` | inherent methods first, then traits in scope; two traits with the same method name → an ambiguity error at compile time |
| `let s: &dyn Shape = &p; s.area()` | a dynamic call through the `conforms` table (a vtable); every other call is direct |
| `enum Shape { Circle { r }, Rect { w, h } }` | a sum Kind with one Kind per variant; `match` tests the variant's Kind (the long-term fix for CRUSH-zorro-37) |
| `struct Wrapper<T> { v: T }` | erased on the VM (values carry their Kind); loud on AOT until monomorphised (decision 2) |

Everything about `Point` is known at compile time. `p.x` compiles to `GET_SLOT 0` and `p.norm2()` to a direct
`CALL`, with no dynamic dispatch, so AOT compiles it immediately.

#### A Python class hierarchy

```python
class Animal:
    kind = "animal"
    def __init__(self, name): self.name = name
    def speak(self): return f"{self.name} makes a sound"
    def __repr__(self): return f"Animal({self.name!r})"

class Dog(Animal):
    def speak(self): return super().speak() + " (woof)"
    def __eq__(self, other): return isinstance(other, Dog) and self.name == other.name
```

```text
KindDecl Animal  (home: python)
  storage:    open_dict                    instances can gain attributes at any time
  semantics:  reference
  parents:    [object] ; resolution: c3
  receiver:   bound_self
  construct:  py_new_init(init: Animal.__init__)
  members:    kind  : static("animal")     class attribute
              speak : method
  hooks:      repr → Animal.__repr__

KindDecl Dog  (home: python)
  parents:    [Animal] ; resolution: c3      MRO [Dog, Animal, object], computed and checked at compile time
  members:    speak : method
  hooks:      eq → Dog.__eq__ ; hash → none   Python rule: defining __eq__ without __hash__ makes instances unhashable
```

| Python code | what happens |
|---|---|
| `d = Dog("rex")` | `CONSTRUCT`: allocate a Record (open dict), run `__init__` with `self` bound |
| `d.speak()` | `CALL_MEMBER`: walk the MRO, find `Dog.speak`, bind `self` |
| `super().speak()` | look up the next class after `Dog` in `type(self)`'s MRO → `Animal.speak`; correct under multiple inheritance and diamonds |
| `f = d.speak; f()` | `GET_MEMBER` on a `bound_self` Kind returns a bound method; under JS's `dynamic_this` the same code loses `this`, as in JS |
| `d.age = 3` · `d.kind` · `d.kind = "x"` | add an instance attribute · read the class attribute through the MRO · shadow it on the instance only |
| `print(d)` | no `__str__`, so it falls back to `__repr__`, as Python does |
| `d == e` · `1 + d` | `OP` → `__eq__` · Python's reflected-operator rule (`__radd__`) from the strategy table |
| `@property` · `@staticmethod` · `@classmethod` | an accessor member · a static member · a method with `receiver: bound_cls` |
| `Dog.bark = lambda self: "!"` | patches the live Kind (version bump keeps caches correct); common enough to support |
| metaclasses, descriptors beyond `property`, `__getattribute__` overrides | **loud** (decision 2) |
| an impossible MRO | a compile error (Python raises `TypeError` at class creation, so a class created conditionally defers the check to run time) |

#### Across languages

A Python `Dog` passed to JavaScript code keeps Python behaviour: `dog.speak()` from JS still walks Python's MRO
and binds `self`, because the operation asks the object's own Kind. When Dart (mixins) or Kotlin arrive, they reuse
the existing C3 or single-inheritance strategies instead of bringing their own runtime.

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
| **K1** | **`crush-cosm` crate** (the COSM schema: `KindDecl`/`InterfaceDecl`, serde like crush-cast, compile-time validation); `Record` + `Kind` runtime values loaded from COSM; `GET/SET/CALL_MEMBER`, `CONSTRUCT`; receiver strategies; **`o.m(x)` stops compiling to a global capability** (methods resolve on the receiver; capability namespaces like `net.fetch` keep their syntax); **Crush gains `impl` + `trait`** as the first COSM producer; struct patterns test the Kind (closes CRUSH-zorro-37's long-term fix) | one language end to end |
| **K2** | the protocols (callable/iterable/eq/hash/str); `OP` dispatch; the HOF prelude in Crush; a VM re-entrancy API so host capabilities *can* call `Func`s | HOFs across languages |
| **K3** | walkers lower to COSM: Python classes (C3, dunders, bound methods, `super`), JS classes/prototypes/`new`/`this` (with CRUSH-250), Rust structs/impl/traits/enums with value semantics, Go structs/methods/embedding/interfaces | each against its parity harness (cpython_corpus, node_corpus, rustc_corpus, a Go corpus) |
| **K4** | static fast paths (`GET_SLOT`, direct calls), inline caches, AOT for static Kinds | performance + AOT exact-or-loud |
| **K5** | cross-language tests: a Python object used from JS in one program, and the reverse | the catalytic thesis |

Each phase lands on the scheduler VM and portable VM first; FastVM/JIT/AOT are loud until they implement it.
The bar is the parity spike's: VM exact, AOT exact-or-loud, never silently wrong.

## Decisions (owner, 2026-10-15)

The owner accepted the recommendations:
1. **Adopt Kinds / COSM** as the object model for Crush and every walker, replacing the JS-specific remainder of
   CRUSH-251 and per-language special cases. Objects are lowered to COSM declarations, like code is lowered to CAST.
2. **Fidelity per language:** the subset each language's parity corpus needs; everything else (Python
   metaclasses/descriptors beyond `property`, JS `Proxy`/`Reflect`/`Object.defineProperty`, Rust generics beyond
   monomorphic use) is **loud** until a strategy is added deliberately.
3. **Rust value semantics:** copy for `Copy` types; move for everything else, with a **loud use-after-move**, as
   rustc does. `.clone()` deep-copies.
4. **Crush-native surface:** Crush grows **`impl` and `trait`** in K1, as the first COSM producer (dogfooding the
   mechanism before other languages depend on it).
5. **GC:** accept leaking instance cycles through K2; **CRUSH-121's root-set fix is a prerequisite of K3**, when
   JS and Python object graphs (cyclic by nature) arrive.

## Bugs found during this survey (independent of this design)

Filed as **CRUSH-zorro-37** and **CRUSH-zorro-38** (P0, #208).

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
