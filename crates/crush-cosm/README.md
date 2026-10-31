# crush-cosm

**COSM — the Crush Object Semantic Model.** The lowered, declarative object model: the object-side
sibling of CAST. Design: [`docs/design/object-model.md`](../../docs/design/object-model.md).

```text
 source ──walker──►  CAST   (functions, statements, expressions)  ──compiler──►  CASM ──► VM / AOT
                ╲
                 ►  COSM   (KindDecl / InterfaceDecl)  ──► Kinds at load time
```

A walker lowers each language's object declarations to `KindDecl`s and `InterfaceDecl`s. What
differs between languages is a choice from a **closed** catalogue of strategies, each implemented
once in the VM. This crate is pure data and algorithms — no VM, compiler or walker code — and has
**no workspace dependencies** (see [Why no `crush-cast`](#why-no-crush-cast)).

## API

| item | what it is |
|---|---|
| `CosmModule { kinds, interfaces }` | one program's object declarations; `kind(name)`, `interface(name)`, `from_json`, `to_json[_pretty]`, `validate()`, `c3_mro(name)`, `resolution_order(name)` |
| `KindDecl { name, home_lang, span, storage, semantics, parents, resolution, receiver, construct, members, hooks, conforms, equality }` | a Kind. `KindDecl::crush(name)` gives Crush's defaults; `field_slot(name)` (slot index for `GET_SLOT`), `member(name)`, `effective_receiver(member)`, `conformance(iface)`, `declared_fields()` |
| `InterfaceDecl { name, home_lang, span, members, satisfaction }` | a trait / interface / ABC; members are `InterfaceMember { name, span, receiver, params, returns, default }` |
| `Storage` | `Sealed { fields }` · `OpenDict` · `OpenDictWithSlots { slots }` (fields are `FieldDecl { name, ty, default, span }`) |
| `Semantics` | `Reference` · `Value(ValueMode::{Copy, Move, CloneExplicit})` |
| `Resolution` | `None` · `Single` · `C3` · `PrototypeChain` · `EmbeddingPromotion` |
| `Receiver` | `DynamicThis` · `BoundSelf` · `BoundCls` · `ExplicitValue` · `ExplicitRef` · `ExplicitRefMut` |
| `Construct` | `Literal` · `Ctor { func }` · `JsNew { ctor_fn }` · `PyNewInit { new_fn?, init_fn? }` |
| `MemberDecl { name, span, member }` / `Member` | `Method { func, receiver? }` (per-member receiver override) · `Field` · `Accessor { get?, set? }` · `Static { value: StaticValue::{Const, Fn} }`. Constructors: `MemberDecl::method`, `method_with`, `field`, `accessor`, `static_fn`, `static_const` |
| `Hooks` | `missing, set, call, iter, op{Op→}, op_reflected{Op→}, eq, hash, cmp, str, repr, clone`, each a `HookImpl::{Fn, Derived, Disabled}` |
| `Conformance { interface, check, members, span }` | "this Kind implements that interface", with its impl table |
| `Equality` / `Satisfaction` | `Identity` · `Structural` / `Nominal` · `Structural` |
| `KindRef`, `InterfaceRef`, `FnRef` | names (`FnRef` = a key of the CAST `Program.functions` the module is lowered alongside) |
| `CosmType` | field/signature types; the JSON twin of `crush_cast::CastType` |
| `validate(&m)`, `validate_with(&m, &[&deps])` | → `Vec<Diagnostic>`; every diagnostic is an error |
| `c3_mro(&kind, &lookup)`, `resolution_order(&kind, &lookup)`, `Linearizer` | resolution orders (`Linearizer` memoizes across many kinds — use it at load) |
| `KindLookup`, `ModuleSet::new(&m, &deps)` | name lookup over a module plus its dependencies (e.g. a prelude declaring Python's `object`) |
| `catalogue::Strategy` | `ALL`, `NAMES`, `as_str()`, `parse()` / `FromStr` for every bare-name strategy; `Storage::from_name`, `STORAGE_NAMES`, `CONSTRUCT_NAMES` — for `@kind(…)`: an unknown value is a `CatalogueError` listing the valid ones |

```rust
use crush_cosm::*;

let mut point = KindDecl::crush("Point");
point.storage = Storage::Sealed {
    fields: vec![FieldDecl::new("x", CosmType::Int), FieldDecl::new("y", CosmType::Int)],
};
point.members.push(MemberDecl::method("norm2", "Point::norm2"));
let module = CosmModule { kinds: vec![point], interfaces: vec![] };

assert!(validate(&module).is_empty());
assert_eq!(module.kind("Point").unwrap().field_slot("y"), Some(1));
assert_eq!(CosmModule::from_json(&module.to_json()).unwrap(), module);
assert_eq!("bound_cls".parse::<Receiver>().unwrap(), Receiver::BoundCls);
```

## JSON shape

Serde JSON, like CAST. Strategy names are the design doc's spellings, `snake_case`:

- **unit strategies** are bare strings: `"reference"`, `"c3"`, `"bound_cls"`, `"literal"`,
  `"open_dict"`, `"identity"`, `"nominal"`;
- **strategies with data** are a one-key object: `{"value": "move"}`,
  `{"sealed": {"fields": [...]}}`, `{"open_dict_with_slots": {"slots": [...]}}`,
  `{"ctor": {"fn": "F"}}`, `{"js_new": {"ctor_fn": "F"}}`,
  `{"py_new_init": {"new_fn": "C.__new__", "init_fn": "C.__init__"}}` (both optional);
- **members** are flat objects tagged by `kind`:
  `{"name": "shift", "kind": "method", "fn": "Point::shift", "receiver": "explicit_ref_mut"}`,
  `{"name": "x", "kind": "field"}`,
  `{"name": "area", "kind": "accessor", "get": "C.area", "set": "C.set_area"}`,
  `{"name": "new", "kind": "static", "value": {"fn": "Point::new"}}`,
  `{"name": "kind", "kind": "static", "value": {"const": "animal"}}`;
- **constants** are plain JSON scalars: `null`, `true`, `3` (Int), `3.0` (Float), `"s"`;
- **hooks** are an object of optional keys; each value is `{"fn": "F"}`, `"derived"` or
  `"disabled"`; operator hooks are keyed by symbol:
  `{"op": {"+": {"fn": "<Point as Add>::add"}}, "op_reflected": {"+": {"fn": "C.__radd__"}}, "eq": "derived", "hash": "disabled"}`.
  Operators: `+ - * / // % ** @ & | ^ << >> < <= > >= neg pos ~ ! [] []= in` (`==` is the `eq`
  hook, ordering is `cmp`);
- **types** are `CastType`'s JSON: `"Int"`, `"Any"`, `{"Array": "Float"}`, `{"Struct": "Point"}`;
- **spans** are `{"file"?, "line", "col", "end_line"?, "end_col"?}` (1-based), optional everywhere;
- empty lists, empty hooks and absent options are omitted; unknown fields are rejected (except
  inside a member, where `kind` is the tag). Parse errors name the JSON path
  (`invalid COSM JSON at `kinds[0].resolution`: unknown variant `mro`, …`).

Full examples — the design doc's worked examples — are in `tests/fixtures/`:
[`rust_point.cosm.json`](tests/fixtures/rust_point.cosm.json),
[`python_animals.cosm.json`](tests/fixtures/python_animals.cosm.json) (validated against
[`python_prelude.cosm.json`](tests/fixtures/python_prelude.cosm.json), which declares `object`),
[`crush_point.cosm.json`](tests/fixtures/crush_point.cosm.json).

## Validation

`validate` / `validate_with` return span-carrying `Diagnostic { code, subject, message, span }`.
`code` is a closed `DiagnosticCode` (stable spelling `cosm::<name>`); `span` is the most specific
one known (member → conformance → field → declaration).

| code | rule |
|---|---|
| `duplicate_decl` | two kinds/interfaces in one module share a name |
| `unknown_parent` · `parent_is_interface` | a parent must be a declared kind |
| `unknown_interface` · `conforms_to_kind` · `duplicate_conformance` | a conformance must name a declared interface, once |
| `duplicate_field` · `duplicate_member` | in storage, a kind's members, a conformance's impl table, an interface |
| `field_shadowed_by_member` | a method/accessor/static member named like a sealed field |
| `undeclared_field` · `empty_accessor` | a `field` member on a sealed kind must be a declared field; an accessor needs `get` or `set` |
| `value_semantics_with_open_storage` | `value(copy)`/`value(move)` with `open_dict`/`open_dict_with_slots` |
| `parent_arity` | `none` takes no parents; `single`/`prototype_chain` at most one |
| `duplicate_base` · `inconsistent_mro` | C3 failures, with CPython's exact `TypeError` message |
| `inheritance_cycle` · `incompatible_parent` | a kind inherits from itself; an `embedding_promotion` kind used as a linear parent |
| `satisfaction_mismatch` | a conformance's `check` must equal the interface's `satisfaction` |
| `missing_interface_member` | every interface member is provided: by the conformance's impl table, by a method/accessor/static member of the kind or an ancestor in its resolution order, or by the interface's `default` |
| `unknown_interface_member` | the impl table provides something the interface doesn't declare |

Each failure is reported once, at the kind where it originates (a child of a kind with a broken
MRO isn't re-reported).

## Resolution orders and C3

`c3_mro` is CPython's `mro_implementation` + `pmerge` (`Objects/typeobject.c`), including the
single-base fast path, the `duplicate base class X` check, and the order in which a stuck merge
lists its bases (`Cannot create a consistent method resolution order (MRO) for bases X, Y`).
C3 is applied at the kind itself; each parent contributes its own `resolution_order`:

- `none` → `[kind]`; `single` / `prototype_chain` → `kind` + its parent's order;
- `c3` → `c3_mro`; `embedding_promotion` → `MroError::NotLinear` (Go promotion is by depth, with
  ambiguity decided per use, so there's no single order to precompute).

`tests/fixtures/c3_cases.json` is **generated by real CPython** (`tests/gen_c3_cases.py`: each
hierarchy is built with `type()` and `__mro__` / the `TypeError` text recorded): the docs'
diamond, both examples from *The Python 2.3 Method Resolution Order*, Wikipedia's example, mixins,
and six inconsistent/duplicate cases. Regenerate with
`python3 tests/gen_c3_cases.py > tests/fixtures/c3_cases.json`; never edit it by hand.

## Why no `crush-cast`

`CosmType` mirrors `crush_cast::CastType` instead of re-exporting it: `crush-cast` depends on
`crush-vm` (through `crush-caison`), and the VM (CRUSH-zorro-43) and CASM (CRUSH-zorro-42) must be
able to depend on this crate without a cycle. The JSON is identical, so a front end holding both
converts with a serde round-trip or a `match`.

## Tests

`cargo test -p crush-cosm`. `COSM_BLESS=1` rewrites the worked-example fixtures after a deliberate
schema change — review the diff, the fixtures are the shape's documentation.
