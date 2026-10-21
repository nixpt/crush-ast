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

#### Crush's own KindDecl

Crush is the first COSM producer (decision 4). Plain `struct` / `impl` / `trait` lower to a `KindDecl` with
**Crush's defaults**:

```text
KindDecl P  (home: crush)
  storage:    sealed { declared fields }
  semantics:  reference                 today's behaviour: no existing program changes meaning
  resolution: none                      traits supply default methods
  receiver:   explicit_ref              `self` is an ordinary first parameter
  construct:  literal                   P { … }; `fn new` is a convention (a static member)
  equality:   structural                as maps compare today
```

```crush
struct Point { x: int, y: int }
impl Point {
    fn new(x, y) { return Point { x: x, y: y }; }
    fn norm2(self) { return self.x * self.x + self.y * self.y; }
}
trait Shape { fn area(self); fn describe(self) { return "a shape"; } }   // default method
impl Shape for Point { fn area(self) { return 0; } }
```

**`@kind(…)` selects any other strategy from the catalogue**, which makes Crush the one language that can write
*any* KindDecl. That's the textual way to test every strategy before the Python/JS walkers lower to COSM, and the way
to write Kind helper libraries in Crush itself:

```crush
@kind(semantics: value(copy))                                   // a Rust/Go-like value type
struct Vec2 { x: float, y: float }

@kind(storage: open_dict, resolution: c3, parents: [Base, Mixin], receiver: bound_self)
struct Widget { }                                               // a Python-like class
```

An `@kind` key or value outside the catalogue is a compile error that lists the valid values. Tickets:
CRUSH-zorro-40..44 (COSM K1).

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

### 9. Method dispatch: how `o.m(args)` resolves

Status: **decides CRUSH-zorro-44** (design pass requested by that ticket's
2026-10-20 status note). This section is the general rule that PRs #285
(CRUSH-277 S3/S5), #286 (CRUSH-280) and the still-open #288 deliberately did
not yet implement: they lower a member call only where the receiver is
provably an instance, and refuse loudly elsewhere. Nothing here changes a
decision in §§1–8; it names, for every receiver, which of the mechanisms
those sections already define applies, and what each backend must do.

**The one invariant.** Static knowledge about a receiver may make a call
*faster* or make it *fail earlier*; it must never change *what the call
resolves to*. A call whose target depends on the receiver's Kind resolves
to the same target whether the compiler proved that Kind, guessed it, or
learned it only at run time. Every rule below is that invariant applied.

**Where the code stands today** (facts, `origin/main` 929662e):

- CAST has exactly one computed-receiver form,
  `Expression::MethodCall { target, method, args }`
  (`crates/crush-cast/src/lib.rs:360-369`), and the frontend compiles it
  unconditionally to CASM `call_member`
  (`crates/crush-frontend/src/compiler.rs:2279-2293`).
- The older dotted form is a `CapabilityCall` named `"obj.method"`. In the
  frontend it becomes `call_member` only when `method` is in the bare-name
  set of `self`-taking methods of the program's Kinds
  (`crates/crush-frontend/src/compiler.rs:2390-2417`,
  `crates/crush-frontend/src/cosm_lower.rs:586-592`); a walker-marked
  member call (`meta.member_call`) that does not so resolve is refused
  before it can reach the capability grant below it
  (`crates/crush-frontend/src/compiler.rs:2364-2388`). An *unmarked*
  dotted call on a declared variable whose method is not a receiver
  method still falls through to `cap_call "<method>"` with the receiver
  as first argument and `method` inserted into the permission set
  (`crates/crush-frontend/src/compiler.rs:2418-2431`) — the legacy path
  §9.6 closes for member-call syntax.
- Both exact VMs dispatch `CALL_MEMBER` through the one shared function
  `cosm_rt::call_member` (`crates/crush-vm/src/scheduler.rs:1767-1774`,
  `crates/crush-vm/src/portable_vm.rs:1516-1523`). On a Record it checks
  an instance field holding a function value *before* the Kind's members
  (`crates/crush-vm/src/cosm_rt.rs:839-843`), then resolves through the
  Kind (`crates/crush-vm/src/cosm_rt.rs:843-898`); on any other value it
  errors `` `.{name}(…)` on a {}: methods resolve on struct instances ``
  (`crates/crush-vm/src/cosm_rt.rs:828-838`). It never calls a capability:
  the doc comment says so (`crates/crush-vm/src/cosm_rt.rs:826-828`), and
  capability inference treats a `CALL_MEMBER` operand as a member name,
  never a capability (`crates/crush-vm/src/cap_inference.rs:19-21`).
- FastVM and the JIT refuse every COSM member op at lowering
  (`crates/crush-vm/src/fastvm/instructions.rs:848-853`, after the
  kind-table gate at `crates/crush-vm/src/fastvm/instructions.rs:425`);
  `crush-aotc` refuses `call_member` by name
  (`crates/crush-aotc/src/codegen.rs:502-510`); the Rust/C AOT backends
  refuse a non-map-compatible kind table up front
  (`crates/crush-aot/src/lib.rs:50-58`) and report any op they cannot emit,
  `call_member` included, as `UnsupportedOps`
  (`crates/crush-aot/src/codegen.rs:1212`). The shared load-time rule is
  `casm::cosm_unsupported_reason_on` at `CosmSupport::Maps` for those
  engines and `CosmSupport::Kinds` for the two exact VMs
  (`crates/casm/src/lib.rs:1046-1085`, `crates/crush-vm/src/cosm_gate.rs:1-11`).

#### 9.1 The dispatch rule, by receiver

A dotted call is classified **once, at lowering, by its syntactic form and
by what the lowering knows about the receiver** — never by the method
name alone, and never re-classified at run time. The classes are disjoint
and are tried in this order:

| # | Receiver | Lowering | Run-time behaviour |
|---|---|---|---|
| 1 | A **capability namespace**: the dotted prefix is not a value binding in scope (`net`, `fs`, `math`, JS `Math`, Python `math`) | A capability call under its full dotted name (`cap_call "net.fetch"`), permission recorded under that name | The capability gate, exactly as today |
| 2 | A receiver of **provably exact Kind** `K`, with `m` resolving to one target on `K` | A direct `call` to the lifted function (`K::m`), receiver passed per `K`'s receiver strategy, arguments adapted (§9.5) | No dispatch; runs on every backend that runs ordinary calls |
| 3 | A receiver whose Kind is **known but not exact** (a `this`, a binding that may hold a subclass, a parameter declared of Kind `K`) | `CALL_MEMBER "m"` | Resolution walks the receiver's actual Kind (§2); §9.5's override check has already proved every reachable override has the adapted shape |
| 4 | A receiver whose Kind is **known only at run time** (an unannotated parameter, a capture, a field read, a call result, a collection element, any computed receiver) | `CALL_MEMBER "m"` (CAST `MethodCall`) | §9.4's fallback, decided by the receiver's actual value |
| 5 | A **literal or otherwise provably primitive** receiver (`[1, 2].push(x)`, `"a,b".split(",")`, a Python `s.upper()`) | The walker/frontend's existing built-in lowering for that language (a VM op or an internal capability-shaped call), never `CALL_MEMBER` | Exactly as today |

Rule 1 is lexical, not semantic: a namespace prefix is not a value, is
never evaluated, and cannot be shadowed by a run-time value. It *can* be
shadowed lexically — if the source binds a value under that name in scope,
class 1 no longer applies and the call is a member call on that value.
That is already the frontend's test: a dotted call is a method call only
when its prefix is a declared variable, and a namespaced capability
otherwise (`crates/crush-frontend/src/compiler.rs:2390-2397`); the JS
walker likewise checks user bindings before its global/namespace table
(`crates/crush-lang-js/src/lower/builtins.rs:75-124` lists the namespace
rows, e.g. `Math.pow` → `math.pow`), and the Python walker treats
`math.floor` & friends as module calls only for its closed `MODULES`
list (`crates/crush-lang-python/src/lower_expr.rs:428`,
`crates/crush-lang-python/src/lower_expr.rs:496-499`).

Rule 2's "provably exact" is deliberately narrow. It covers: a
construction expression (`new C(…)`, a struct literal `P { … }`) used
directly as the receiver; `super.m(…)` (the target ancestor is named by
the lexically enclosing class — the JS walker already lowers it to a
direct call, `crates/crush-lang-js/src/lower/class.rs:270-336`); a static
member called on its Kind's name; and a Crush receiver whose Kind has
`resolution: none` and no subclassing, where the declared Kind *is* the
exact Kind. It does **not** cover `this` in a subclassable hierarchy, a
reassignable binding, or anything a nested function can assign — the JS
walker already forgets a binding's Kind in exactly those cases
(`crates/crush-lang-js/src/lower/expr.rs:743-765`). Where rule 2 does not
apply but the Kind is known, rule 3 applies instead: same target, found
at run time. **Direct lowering is an optimisation, not a second
semantics** (§6's static fast paths); an implementation slice that emits
`CALL_MEMBER` for a rule-2 receiver is conforming, if slower.

Rule 5 keeps the walkers' existing primitive behaviour byte-for-byte:
the JS builtin table's one live receiver row is `push`
(`crates/crush-lang-js/src/lower/builtins.rs:128-130`; the string/array
rows below it are `Planned`, i.e. loud at walk time), the computed-receiver
path already gives those rows first claim on member-access and call-result
receivers (`crates/crush-lang-js/src/lower/expr.rs:859-875`), and the
Python walker translates a method on a value to a named Crush function
such as `str.to_upper` (`crates/crush-lang-python/src/lower_expr.rs:430-455`).
Rule 5 applies only where the *walker* knows the receiver is a primitive
of its own language; a receiver it cannot so classify is rule 4, even if
the value turns out to be a primitive at run time (§9.4).

*Reason.* Classification by form and provenance is checkable at lowering,
keeps capability namespaces closed (§9.6), and makes the run-time fallback
the single place where an unknown receiver is interpreted — one shared
implementation, as CONTRIBUTING requires, instead of one guess per walker.

*Rejected alternatives.* (a) **Classify by method name** (a global table:
`push` is always the array builtin, `add` always a Record method): this is
today's bug class — the name, not the receiver, decides — and it cannot
express §9.3's clash rule. (b) **Lower every dotted call to `CALL_MEMBER`,
including namespaces**: a namespace has no value to dispatch on, so
`net.fetch` would need a fake receiver value and a capability hidden
inside the member op, breaking §9.6. (c) **Keep unknown identifier
receivers loud forever** (the #285 D4 position as a permanent rule):
safe, but it makes `fn apply(o) { o.m() }` inexpressible for Records,
which is precisely zorro-44's remaining problem; loudness moves to the
actual failure (a receiver with no such member), which is the honest
place for it.

*Worked example.*

```crush
struct Point { x: int, y: int }
impl Point { fn norm2(self) { return self.x * self.x + self.y * self.y; } }
fn via_param(p) { return p.norm2(); }        // rule 4 (today: also rule 3's set)
let p = Point { x: 3, y: 4 };
print(p.norm2());                            // rule 3, or rule 2 as an optimisation
print(via_param(p));                         // rule 4
print(Point { x: 1, y: 2 }.norm2());         // rule 2 eligible
```

`p.norm2()` lowers to `call_member "norm2" 0` (as it does today,
`crates/crush-frontend/src/compiler.rs:2400-2416`); `via_param`'s body
lowers the same way under this section, where today a walker would refuse
a parameter receiver. Both print `25`/`5` via the receiver's Kind on the
two exact VMs; a backend that cannot dispatch (§9.4) refuses the
`via_param` program at load/compile time, never at the `print`.

#### 9.2 Name clashes: a Record method named like a built-in

**Rule.** Precedence is a property of the *receiver's value*, in this
order, and of nothing else:

1. an instance field holding a value (§9.3) — it shadows everything;
2. the receiver Kind's own members, then its resolution order, its
   conformances' impl tables and its interfaces' default methods (§2's
   lookup, `crates/crush-vm/src/cosm_rt.rs:21-24`);
3. the built-in method table for the receiver's primitive type (§9.4) —
   reachable only when the receiver is not a Record/Kind at all, so in
   practice this tier never competes with tiers 1–2;
4. otherwise, a loud error (§9.4).

So **a Record's own method always wins over a built-in of the same name**
(`add`, `get`, `push`, `clone`, …), and a primitive receiver always gets
its built-in: the two never race, because tier 3 is entered only for
non-Record receivers. The decision is taken **statically whenever the
receiver's Kind is statically known** — the lowering must consult the
Kind before any built-in table, so a known-Record call can never be
captured by a built-in row — and **at run time otherwise**, by the same
precedence in the shared `CALL_MEMBER` implementation. By §9's invariant
the two must agree; a lowering that lets a built-in row claim a receiver
it merely *cannot prove* is a Record is permitted only because the
run-time tier order then still applies to whatever value arrives — and
where a walker does that today for computed receivers
(`crates/crush-lang-js/src/lower/expr.rs:859-875`), a later slice must
restrict first claim to receivers the walker cannot prove are instances,
as it already does for `new C()` results
(`crates/crush-lang-js/src/lower/expr.rs:866-875`).

Two existing precedents are this rule, not exceptions to it: an instance
field shadows an inherited accessor on read
(`crates/crush-vm/src/cosm_rt.rs:690-696`), and `clone` is a built-in
only when the Kind declares no `clone` member
(`crates/crush-vm/src/cosm_rt.rs:896-898`).

**A non-Record receiver reaching a member call** is §9.4: if the shared
built-in table has a row for *(that value's type, that name)*, it runs;
if the name is a built-in only for some *other* type (`"hi".push(x)`,
`[1].upper()`), or for no type, the call errors loudly naming the method
and the receiver's type. It never reads as `null`, never becomes a plain
function call, and never becomes a capability (§9.6).

*Reason.* Receiver-first precedence is what every hosted language does
(JS property lookup before any global, Python attribute lookup before
any builtin function), and it is the only rule under which losing static
information (inlining a function, passing a value through a collection)
cannot silently change a program's meaning.

*Rejected alternatives.* (a) **Built-in wins on name clash** (keeps
primitive programs stable when a Kind grows a method): silently changes
existing Record programs the day someone adds `impl Bag { fn add … }`.
(b) **Static-only resolution, error if the Kind is unknown**: makes the
same program legal or illegal depending on provenance tracking, and
bans §9.1 rule 4 outright. (c) **Warn/fall through to the built-in when
a Record has no such method**: a typo'd method name on a Record would
mutate an array builtin instead of erroring — silently wrong.

*Worked example.*

```javascript
class Bag { add(x) { return "bag:" + x; } }   // JS slice, CRUSH-277
let b = new Bag();
b.add(1);          // "bag:1" — tiers 1–2; the walker's Map/Set `add` row
                   // is Planned/loud and must not claim this receiver
let xs = [1, 2];
xs.push(3);        // tier 3 / rule 5 — the array built-in, as today
function id(v) { return v; }
id(b).add(1);      // rule 4 → CALL_MEMBER → tier 2 at run time: "bag:1"
id(xs).push(3);    // rule 4 → CALL_MEMBER → tier 3 at run time: appends
id(xs).add(1);     // loud: no `add` method on an array value
```

#### 9.3 A field holding a function value: `o.f(x)`

**Rule.** Calling a member that resolves to an instance field is calling
the value stored there: `GET_MEMBER` + `CALL_VALUE`, in that order, with
**no receiver injected**. Captures/binding already inside the function
value are its leading arguments, exactly as `CALL_VALUE` passes them
(`crates/crush-vm/src/vm.rs:177-189`). This holds for a Record field and
for a `Value::Map` entry alike; a Map has no Kind and no methods, so a
field function is the *only* thing `m.f(x)` can mean on a Map. If the
field is absent, the error is the member error (`has no member`/
`has no method`); if it is present but not callable, the error is
`CALL_VALUE`'s not-callable error. A bound method obtained by reading a
method member (§2: `GET_MEMBER` under `bound_self` returns a function
value already carrying its receiver,
`crates/crush-vm/src/cosm_rt.rs:570-589`) is indistinguishable from a
stored lambda at the call site, and needs no special case.

In lowering, `o.f(x)` is **not** special-cased to a literal
`GET_MEMBER`+`CALL_VALUE` pair unless the compiler can prove `f` is a
field (it generally cannot); it lowers to `CALL_MEMBER "f"`, whose
Record branch already implements exactly this order — own field first,
callable-checked through the same `callable` helper `CALL_VALUE` uses
(`crates/crush-vm/src/cosm_rt.rs:839-843`). The one new runtime case this
section adds is the Map branch of §9.4. Keeping one lowering means
`o.f(x)` and `let g = o.f; g(x)` differ only in the documented way: the
second has already paid `GET_MEMBER`'s binding policy (a `dynamic_this`
method read off its object loses `this`, §2), the first has not.

*Reason.* `GET_MEMBER`+`CALL_VALUE` is the semantics the ticket asks for
and the one the Record path already implements; inventing a receiver
injection for stored functions would make a field lambda behave unlike
the same lambda in a variable.

*Rejected alternatives.* (a) **Inject the receiver as first argument**
(Python-style binding for stored functions): wrong for Crush/JS, where a
stored function is not a method; binding is a property of *method
members* under a `bound_*` receiver strategy, decided by `GET_MEMBER`,
not by the call syntax. (b) **Refuse field-function calls unless the
field's type is known statically**: Crush fields are not typed that way,
and the run-time callable check already errors precisely.

*Worked example.*

```crush
let handlers = { run: fn(x) { return x + 1; } };
print(handlers.run(41));        // Map field → 42
struct S { f: any }
let s = S { f: fn(x) { return x * 2; } };
print(s.f(21));                 // Record field → 42 (today's Record branch)
print(s.g(1));                  // loud: `S` has no member `g`
let t = { run: 3 };
print(t.run(1));                // loud: `run` is not callable
```

#### 9.4 The runtime fallback: `CALL_MEMBER` on a non-Record

**Rule.** The shared implementation behind `CALL_MEMBER`
(`cosm_rt::call_member`, extended in place — not a second copy per VM)
dispatches on the receiver value, in §9.2's order:

| Receiver value | Behaviour |
|---|---|
| `Record` | §9.3's field check, then the Kind lookup; missing method → `` `{Kind}` has no method `{name}` `` (today's error) |
| `Kind` (a class as a value) | Static members, as today (`crates/crush-vm/src/cosm_rt.rs:829-831,872-888`) |
| `Map` | §9.3's field check only; absent → a "no member/method" error naming the map and member; present non-function → not-callable |
| Array, string, and the other primitive types | The **built-in method table**: a closed, per-type table of (type, name, arity) → the VM-internal implementation those methods already use as ops/internal calls. No row → loud error |
| Anything else (`int`, `bool`, `null`, a function value, …) | Loud error in today's shape: `` `.{name}(…)` on a {type}: … `` (`crates/crush-vm/src/cosm_rt.rs:832-837`), extended to say the built-in table was also consulted |

The built-in fallback table is **not** the capability registry: it is
keyed by the receiver's type (a name-only match is the §9.2 bug), it is
implemented once beside `cosm_rt` and called by both exact VMs (the
`io_print.rs`/`io_read.rs` pattern CONTRIBUTING mandates), it records no
permission, and it can only perform operations that are already exact,
in-process VM operations on that value (the array `push`/`append` family
the scheduler already implements by type-checking its first argument,
`crates/crush-vm/src/scheduler.rs:2236-2244`, is the model). A method
whose implementation would need a host capability, a callback into user
code (the K2 HOFs), or a language runtime is **not** a fallback row; it
stays loud with the area ticket's name, as the JS table's `Planned` rows
already are (`crates/crush-lang-js/src/lower/builtins.rs:131-191`).

**Backends, exactly:**

| Backend | `CALL_MEMBER` (any receiver) |
|---|---|
| Scheduler VM | Implements this section (shared `cosm_rt`) |
| Portable VM | Implements this section (the same shared `cosm_rt`) |
| FastVM | **Refuses at lowering**, as today — on the bytecode path by name (`crates/crush-vm/src/fastvm/instructions.rs:848-853`) and on the CASM path as an unknown op (`crates/crush-vm/src/fastvm/instructions.rs:1447-1448`); a program of rule-2 direct calls and rule-5 built-ins, with no `CALL_MEMBER`, is unaffected |
| JIT | **Refuses**, through the same FastVM lowering it is built on; it must not fall back to a partial (built-in-only) dispatch |
| AOT (Rust, C, `crush-aotc`) | **Refuses at compile time**, naming `call_member`/COSM, as `crush-aotc` already does by name (`crates/crush-aotc/src/codegen.rs:502-510`) and the Rust/C backends do via `UnsupportedOps` (`crates/crush-aot/src/codegen.rs:1212`); a map-compatible kind table alone is not a refusal — only the op is. PTX and any other backend fall under the "future backend" row below until they do the same |
| Any future backend | Implements all four receiver tiers, or refuses any program containing `call_member` at load/compile time. A partial implementation (Records only, or built-ins only) is non-conforming |

No backend may implement the fallback by routing it to `cap_call`: the
fallback is below the capability gate by construction (§9.6), and the
kind-table gate (`CosmSupport`) is unchanged — FastVM/JIT/AOT keep
refusing Kinds a plain map cannot represent
(`crates/casm/src/lib.rs:870-916`), independently of this section.

*Reason.* The fallback's whole value is that an unknown receiver gets
*the same answer* the static path would have given. That answer is only
well-defined if it lives in one shared function (§9's invariant); and a
backend that can only answer for *some* receiver values must refuse the
op outright, because the receiver value is exactly what it cannot know
at compile time — the ticket's exact-or-loud bar.

*Rejected alternatives.* (a) **Fall back to the global capability named
`m`** (today's unmarked legacy path): turns a typo or a missing method
into a host-capability grant — the bug PR #285's review removed for
marked calls; §9.6. (b) **Return `null` for a missing member** (the lax
`GET_FIELD` convention): a missing *method* is a programming error, and
JS/Python both raise; the lax read stays for field *reads* only. (c) **A
per-VM fallback table**: the CRUSH-114 divergence class (`len()` on
strings) — two tables will drift.

*Worked example.*

```crush
fn bump(v) { return v.push(9); }   // parameter receiver: rule 4
bump([1, 2]);                      // array → built-in tier → [1, 2, 9]
struct B { n: int }
impl B { fn push(self, x) { return "B.push"; } }
bump(B { n: 1 });                  // Record → Kind tier → "B.push"
bump(42);                          // loud: `.push(…)` on an int
bump({ push: 1 });                 // loud: field `push` is not callable
```

On the scheduler and portable VMs all five lines behave as annotated.
On FastVM, the JIT and AOT, the program is refused before it runs
(`call_member` is present), even though every actual call would have
succeeded on two of them — that is the accepted cost of exact-or-loud,
and rule-2/rule-5 programs (where the lowering never emits
`CALL_MEMBER`) are the way to reach those backends today.

#### 9.5 Argument shapes: default and rest parameters under dynamic dispatch

**Rule. The CRUSH-280 refusal stays.** A dynamically dispatched call is
passed **raw** (the source arguments, nothing synthesised) unless the
lowering knows the target signature exactly enough to adapt it, and
adaptation is positional and therefore only sound when every target the
call can actually reach has the *same* signature:

- **Exact target** (rule 2, `super`, a Kind's static, `new C()`): adapt
  against that signature — hidden `__has_<param>` flags appended after
  the parameters, a rest parameter collected into one array
  (`crates/crush-lang-js/src/lower/scope.rs:48-57`,
  `crates/crush-lang-js/src/lower/expr.rs:951-1016`). Unchanged.
- **Known Kind, inexact receiver** (rule 3): adapt against the declared
  Kind's signature, and at end of lowering verify that every subclass
  overriding that member resolves it to the identical signature; any
  mismatch is a lowering error. This is the check that exists
  (`crates/crush-lang-js/src/lower/class.rs:169-206`), and this section
  makes it the rule for every walker, not a JS-local one.
- **Unknown receiver** (rule 4): pass raw, and at end of lowering verify
  that **no member of that name anywhere in the program has a
  non-plain (default/rest) signature**; if one does, refuse at lowering
  with the existing message ("the arguments could land in the hidden
  slots", `crates/crush-lang-js/src/lower/class.rs:208-223`). The same
  scan covers a Record method that would shadow a built-in: an unknown
  receiver that turns out to hold a primitive therefore also refuses —
  conservative, loud, accepted.
- **Field functions** (§9.3): always raw; a function value's arity is
  checked by `callable` at run time (`crates/crush-vm/src/vm.rs:179-189`).
  Function values with default/rest parameters are already refused at
  the point they are used as values
  (`crates/crush-lang-js/src/lower/expr.rs:41-47`), so no adapted shape
  can reach this tier.
- **Built-in fallback rows** (§9.4): plain fixed signatures only, checked
  against the row's arity when the fallback runs.

At run time nothing changes: `CALL_MEMBER` keeps checking the resolved
function's exact arity against `argc` plus its leading (receiver/captured)
arguments (`crates/crush-vm/src/cosm_rt.rs:844-854`), so a raw call that
reaches a plain-signature method with the wrong count fails loudly there,
and an adapted call's hidden flags can only reach a target proved to
share the shape they were built for.

*Reason.* The hidden-flag ABI is positional: a call adapted for one
signature and dispatched to another does not error, it *misbinds*
(flags land in user parameters, defaults silently trigger) — the one
failure mode this repo's bar forbids outright. Since `CALL_MEMBER`'s
operand carries only a name and a raw `argc`
(`crates/crush-vm/src/bytecode.rs:204-206`), the call site cannot
describe an unadapted shape to the callee; until the ABI can, refusal
is the only exact option.

*Rejected alternatives.* (a) **Run-time shape check on the raw call**
(compare `argc` against the resolved signature and synthesise
defaults/rest then): impossible without changing what the instruction
carries — the callee's default *expressions* live in the callee, but
deciding "the caller omitted parameter 2" from a raw stack requires the
instruction to say which source arguments were given, and rest/default
mixes make that a per-signature descriptor, i.e. a new instruction or a
wider operand on every backend. (b) **Per-method raw-entry trampolines**
(each non-plain method grows a plain-ABI wrapper that fills its own
defaults/rest, and dynamic dispatch targets the wrapper): sound, and the
likely eventual design, but it changes how every lifted method is named
and called (including the direct path and the AOT lifted-method calls)
— a slice of its own, gated on whether dynamic calls to such methods are
actually wanted; see Open (owner) below. (c) **Adapt speculatively for
the most likely Kind**: silently wrong on the other Kinds, by
construction.

*Worked example (JS slice).*

```javascript
class A { m(a, b = 10) { return a + b; } }
class B { }                        // no `m` anywhere else, non-plain or not
function f(o) { return o.m(1); }   // rule 4, unknown receiver
f(new A());
```

Today this is refused when lowering finishes, because a member `m` with
a default exists in the module; under this section it stays refused, on
every backend, before any code runs. `new A().m(1)` (exact) adapts and
returns `11`; a `B.m` override with signature `(a, b)` added later makes
even a rule-3 call through an `A`-typed receiver a lowering error,
rather than a misbound call.

#### 9.6 Capabilities: no member call can become a host capability

**Rule.** Member calls and capability calls are disjoint at every layer,
by construction rather than by check:

1. **Syntax.** A capability is named, never computed: a namespace call
   (§9.1 rule 1) is a `CapabilityCall` whose full dotted name is the
   capability; a member call is a `MethodCall`, or a dotted
   `CapabilityCall` *marked* `member_call` by its walker. Walkers must
   mark every receiver call they emit in dotted form — the JS walker
   does (`crates/crush-lang-js/src/lower/expr.rs:828-840`) — and the
   frontend refuses a marked call that does not resolve as a member
   before the capability grant can see it
   (`crates/crush-frontend/src/compiler.rs:2364-2388`).
2. **Compilation.** `call_member` records no permission and emits no
   `cap_call`. The one place the frontend historically turned
   `obj.method` into `cap_call "method"` + a recorded permission
   (`crates/crush-frontend/src/compiler.rs:2418-2431`) is reachable, after
   the zorro-44 implementation, only from the *unmarked* parser idiom —
   and that idiom, applied to a declared-variable receiver, is a member
   call by §9.1 and must be lowered as one (`call_member`). After that
   change no compiler path takes a receiver call to `cap_call`; a
   capability name can only originate from a namespace form (rule 1) or
   a bare capability form, both of which name it in full.
3. **Bytecode.** `CALL_MEMBER`'s operand is a member name
   (`OperandKind::Member`), not a capability
   (`crates/crush-vm/src/cap_inference.rs:19-21`); capability inference
   therefore never reports a member name as a used capability, and —
   because dispatch is dynamic — it treats every function the kind table
   names as reachable, so the capabilities a *method body* uses are
   still all reported (`crates/crush-vm/src/cap_inference.rs:229-236` has
   the shape of this test). A method that calls `io.print` needs that
   grant for its body; the act of calling the method needs none.
4. **Run time.** `cosm_rt::call_member` and its §9.4 fallback contain no
   capability lookup, no permission check and no host-cap call; the
   fallback table (§9.4) is VM-internal operations only. The scheduler's
   capability dispatcher is reachable only from `CAP_CALL`
   (`crates/crush-vm/src/scheduler.rs:2051-2060` is its gate).

*Reason.* PR #285's review found a real path from an unresolved member
call to a granted host capability (fixed by the `member_call` guard
above). A guard is a check; this rule makes the two name spaces
unrepresentable in each other's instructions, so the next walker or
frontend path cannot reintroduce it by forgetting a flag: to reach a
capability, a program must contain a `cap_call` naming it in full, and
no lowering of a receiver call emits one.

*Rejected alternatives.* (a) **Keep the guard, skip the lowering change**
(status quo): the unmarked parser idiom remains a live counterexample —
any future walker emitting it unmarked regains the grant. (b) **Let the
runtime fallback consult the capability registry for names no member
matches** ("convenient" host methods): reintroduces ambient authority
through the receiver — a value from another language could name host
capabilities the program never declared — and violates the
capability-security rule in CLAUDE.md outright.

*Worked example.*

```crush
struct P { x: int }
let p = P { x: 1 };
p.frobnicate();     // no such method: compile- or run-time refusal
                    // naming `P`/`frobnicate`; no permission is recorded
net.fetch("https://example.invalid");   // rule 1: cap_call "net.fetch",
                    // permission "net.fetch", gated as today
let net = { fetch: fn(u) { return "local"; } };
print(net.fetch("x"));                  // `net` is a value here: §9.3
                    // field-function call → "local"; never a capability
```

#### 9.7 Open (owner)

The following are **not** decided above; each names the recommendation
the implementation should follow until the owner rules, so no slice
blocks on them:

1. **Open (owner) — the initial built-in fallback inventory (§9.4).**
   Recommendation: only (type, name) rows whose implementation is already
   exact on both exact VMs (the array `push`/`append`/`pop` family and
   the string rows the Python/JS walkers already lower), each with a
   parity test; every other name stays loud with its area ticket.
   Adding a row is adding language surface — should Map/Set `get`/`set`/
   `add`, array `map`/`filter` (K2 HOFs), or Python dict methods join the
   *runtime* table, or remain walker-lowered only?
2. **Open (owner) — field-function receiver binding for `dynamic_this`
   Kinds (§9.3).** Recommendation: keep "no receiver injected" for
   Crush, Map and Record field values, matching `GET_MEMBER` +
   `CALL_VALUE` literally. JavaScript object-literal methods
   (`let o = { m() { return this; } }; o.m()`) want `this === o`; that
   is a property of a future `js.Object` Kind's *member* lookup (K3),
   not of field values generally. Owner to confirm the split before K3
   lowers object literals.
3. **Open (owner) — lifting the §9.5 refusal.** Recommendation: keep the
   refusal until a concrete program needs a dynamic call to a
   default/rest method. If that day comes, the trampoline design
   (§9.5, rejected alternative (b)) is the candidate; it touches method
   naming on every backend, including AOT's lifted-method calls, and
   should be its own ticket with its own design note.
4. **Open (owner) — whether FastVM/JIT/AOT ever implement `CALL_MEMBER`.**
   Recommendation: not in zorro-44; §6 already starts AOT dynamic member
   ops loud, and rule-2 direct calls give static programs a path to
   every backend. Owner to say whether K4's static fast paths should
   grow into full dynamic dispatch on those backends, or whether
   dynamic-receiver programs are permanently VM-tier programs.

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
