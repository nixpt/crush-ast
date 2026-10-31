# CRUSH-zorro-43 — COSM K1.4 — VM runtime: Records, Kinds, member ops, CONSTRUCT, receivers

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-43 |
| **Priority** | P1 |
| **Status** | Done (PR #222 merged 2026-10-15) |
| **Phase** | COSM K1 |
| **Assignee** | vega |
| **Dependencies** | CRUSH-zorro-42 |
| **Estimated effort** | L |
| **Filed by** | vega — 2026-10-15, COSM phase K1 (owner decisions recorded in the design doc and dejavue) |

## Problem

The scheduler VM and the portable VM gain the runtime half of COSM: Kinds instantiated from the program's kind
table, `Record` values that point at them, and the generic member ops. This is also the real fix for
CRUSH-zorro-37: a struct pattern tests the value's Kind.

## Success criteria

- [x] `Value::Record(Rc<RecordValue>)` with `kind` + slots laid out by the Kind's sealed fields (an overflow map for `open_dict`), and `Kind` values built from `CosmModule` at load (C3 MRO precomputed via `crush_cosm::c3_mro`).
- [x] New CASM ops + CVM1 opcodes, on **both** the scheduler and the portable VM (crush-web):
  - `GET_MEMBER`, `SET_MEMBER`, `HAS_MEMBER`, `DEL_MEMBER`;
  - `CALL_MEMBER name argc`;
  - `CONSTRUCT kind argc`;
  - `GET_SLOT i` / `SET_SLOT i` (static fast path).
- [x] Lookup follows the Kind's `resolution` (`none` and `c3` in K1; `prototype_chain`/`embedding_promotion` may be loud until their walkers arrive).
- [x] Receiver strategies: `explicit_value` (copy for value Kinds) / `explicit_ref` / `explicit_ref_mut` / `bound_self` (GET_MEMBER returns a bound method: a `Func` with a receiver slot) / `bound_cls` / `dynamic_this` (basic).
- [x] Value semantics: `value(copy)` Records copy on assignment, argument passing and return (the compiler emits the copies; the VM provides `COPY`), and `clone` deep-copies. `value(move)` use-after-move checking is compile-time and can come later (it's Rust-only); for now treat `move` like `copy` at run time and document it.
- [x] `TYPEOF` of a Record returns its Kind name; printing uses the `str`/`repr` hooks if present, else Crush's struct print.
- [x] **Struct patterns test the Kind** (`match v { P { … } => …, Q { … } => … }`), which **closes CRUSH-zorro-37**: add the zorro-37 program as a test.
- [x] Tests on both VMs (`run_both`-style): construction, field get/set, method calls with each receiver strategy, a C3 diamond lookup through `@kind(resolution: c3, parents: …)` from zorro-41, `value(copy)` vs `reference` aliasing, bound methods extracted as values, pattern matching on Kind.

## Resolution

**Approach (b), at the CASM → CVM1 boundary** (recorded in dejavue). Every declared struct is a Record on the
scheduler VM and the portable VM. The CASM a struct literal compiles to is still `new_obj` + `set_field`, now tagged
`{"kind": "P"}`: FastVM, the JIT and AOT ignore the tag and build the same map as before, so map-compatible
programs run and agree on every engine (`crush-aot/tests/cosm_k1_lifted_methods.rs`, whose map-engine tests are
unchanged). `casm_to_vm` lowers a tagged `new_obj` to `NEW_RECORD` (no constructor runs: the literal writes the
fields), `new P()` and `C(…)` to `CONSTRUCT`. `GET_FIELD` / `SET_FIELD` on a Record are the member ops
(`GET_FIELD` keeps a map's `null` for a missing name; `GET_MEMBER` reports it). Approach (a) was rejected: default
structs would carry no Kind, so zorro-37 would stay open.

- **Runtime** (`crush_vm::cosm_rt`): `Value::Record(Rc<RecordValue>)` (slots by the Kind's declared fields, unset
  until written; an overflow map for open storage; fields print in first-written order, exactly as the map a struct
  was) and `Value::Kind`. `KindTable::load` builds the Kinds at load with their resolution order precomputed
  (`crush_cosm::resolution_order`, i.e. `c3_mro` for `c3`); an inconsistent MRO is refused before anything runs.
- **CVM1 opcodes 0xC0–0xCA** on both VMs: `GET_MEMBER`, `SET_MEMBER`, `HAS_MEMBER`, `DEL_MEMBER`,
  `CALL_MEMBER "m" argc` (arguments last-first, receiver on top), `CONSTRUCT "K" argc`, `GET_SLOT i`, `SET_SLOT i`,
  `IS_KIND "K"`, `COPY`, `NEW_RECORD "K"`. Matching CASM ops (`get_member` … `copy`) and `casm::OpCode` variants.
  Programs with Kinds record each function's arity in the CVM1 function table (`FunctionEntry.arity`,
  `.func name arity` in assembly), which bound methods need.
- **Resolution**: `none`, `single`, `c3` (instance field → inherent members → conformance impl tables → interface
  defaults, per Kind in the MRO; two traits providing a name is an ambiguity error). `prototype_chain` and
  `embedding_promotion` are refused at load.
- **Receivers**: `explicit_ref` / `explicit_ref_mut` / `bound_self` / `dynamic_this` pass the Record,
  `explicit_value` a copy (value Kinds), `bound_cls` the Kind. `GET_MEMBER` of a method: bound to the instance
  (`bound_self`) or the Kind (`bound_cls`), `this = null` (`dynamic_this`), the plain function (explicit receivers).
- **Construction**: `literal` (no arguments, or one per field), field defaults, `ctor(f)` / `js_new(f)` /
  `py_new_init` (an inherited `__init__` is found through the MRO). A constructor that is a static member is a
  factory (its result is the instance); otherwise it initialises the new instance (`self`) and a `POP` after
  `CONSTRUCT` drops its return value (`casm::kind_constructor`, shared by the compiler and the VMs). A Python
  `__new__` is refused at load.
- **Value semantics**: the compiler emits `copy` on assignment, argument passing, `return` and field writes when the
  program declares a `value(copy|move)` Kind. **`value(move)` behaves as `copy` at run time** (use-after-move is a
  compile-time check, still to come). `clone()` deep-copies, or calls the Kind's `clone` hook.
- **`TYPEOF`** of a Record is its Kind's name. **Printing** goes through the `str` hook, else `repr`, for
  `PRINT` and the text capabilities (`io.print`, `str.concat`, …), top-level and inside arrays/maps; the hook runs
  to completion in a nested run of the program. Otherwise Crush's struct print.
- **Struct patterns test the Kind** (`is_kind` before the fields), closing CRUSH-zorro-37; a struct pattern
  naming no declared struct is a compile error (which keeps the Python walker's class patterns loud until Python
  classes exist).
- **`p.m(…)`** (a method declared in an `impl`, on a variable receiver) compiles to `call_member`: it resolves on the
  Kind on the VMs and is never a global capability. A non-Record receiver is an error. What zorro-44 adds (a
  fallback to built-in methods for non-Records, static direct calls, chained receivers) is untouched.
- **The shared rule is per engine** (`casm::CosmSupport::{Maps, Kinds}`, `kind_unsupported_strategies_on`,
  `cosm_unsupported_reason_on`, defined once). The VMs (`cosm_gate`, now `Kinds`) refuse only prototype chains,
  Go embedding, a Python `__new__`, accessors and hooks other than `str`/`repr`/`clone`. The frontend refuses
  the same set at compile time. FastVM/JIT/AOT keep the `Maps` level and refuse the member ops they can't run;
  FastVM refuses `type_of` in a program with Kinds (it would name a map). crush-aotc refuses the member ops
  explicitly.
- **Sealing holds on every engine** (review of #222). A write of a field a `sealed` Kind doesn't declare
  (`a.z = 5` on `struct P { x }`) fails at run time with one message,
  `` `P` is sealed: it has no field `z` (fields: x) `` (`casm::sealed_field_error`). The VMs refuse it on the
  Record. The map engines enforce it at run time on the struct maps they build from the tagged `new_obj`
  (option (a) on each): FastVM and the JIT give the map a class that names the Kind's fields
  (`fastvm::sealed_class` / `check_sealed_write`; the JIT reports it with error flag 5 and the message); AOT
  Rust's `OrdMap` and AOT C's `CrushObject` carry the Kind's field list and exit with the message on a write
  outside it. crush-aotc still refuses objects. Test: `crush-aot/tests/cosm_k1_lifted_methods.rs::undeclared_write`.
- **Equality decision.** `structural` equality compares the set fields and **ignores the Kind**:
  `P { x: 1 } == Q { x: 1 }` and `P { x: 1 } == {x: 1}` are `true`. That is the design doc's "structural — as maps
  compare today", keeps every engine agreeing (struct instances are maps on FastVM/JIT/AOT), and Rust/Go only
  compare values of one type anyway. A Kind that must not equal another declares an `eq` hook (K2) or
  `identity`.
- Tests: `crush-lang-sdk/tests/cosm_k1_records.rs` (both VMs: construction, field get/set, each receiver, C3
  diamond, value vs reference, bound methods, constructors, identity equality, Kind patterns, the zorro-37
  program), `crush-vm/src/tests/records.rs` (hand-written CVM1: has/del, slots, defaults, positional construct,
  `IS_KIND`/`COPY`/`TYPEOF`, `str`/`repr` hooks, Python-style init, MRO and ambiguity errors, disassembly, FastVM
  refusals).

## Technical approach

- Keep `Value::Map` for plain dictionaries and object literals; only declared types become Records.
- Inline caches are K4; correctness first.

## Files to modify

- `crates/crush-vm/src/{vm.rs,scheduler.rs,portable_vm.rs,bytecode.rs}`
- `crates/casm/src/lib.rs`
- `crates/crush-lang-sdk/src/compile.rs`
- `crates/crush-frontend/src/compiler.rs` (pattern match on Kind)

## Non-goals

- FastVM/JIT/AOT execution (they stay loud via zorro-42).
- Operator hooks / `OP` (K2).

## Dispatch

**Read first:** `docs/design/object-model.md` (PR #206: **COSM — the Crush Object Semantic Model**), especially *COSM: objects are lowered, like code*, *The strategy catalogue*, *Worked
examples* and *Decisions*. If #206 isn't merged yet:
`git show origin/agent/vega/crush-kinds-design:docs/design/object-model.md`.

**The bar** (as for the parity work): the scheduler VM and the portable VM are exact. FastVM, JIT and AOT either
implement the feature or refuse **loudly at load/compile time**. Nothing is ever silently wrong, and anything
unsupported names what isn't supported.

**Rules:**
- Work in a worktree under `crush-ast/.jagent/worktrees/<you>` (a sibling of the `buckets` symlink).
- `CARGO_TARGET_DIR=/workspace/scratch/builds/<you>`, `CARGO_BUILD_JOBS=6` (14 GB RAM shared).
- Commit trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- A ready-for-review PR; never force-push or merge.
- If GitHub shows a conflict only in `.dejavue/*` (merge=union), merge `origin/main` locally.
- **Update the parity harness lists in the same PR** if your change flips any corpus entry
  (`crates/crush-lang-js/tests/node_corpus_test.rs`, `crates/crush-aot/tests/js_parity.rs`, `python_parity.rs`).
- Gates: the crates you touch, plus `-p crush-vm -p crush-lang-sdk -p crush-frontend -p crush-aot`, the walker set
  (the `Test (walkers)` CI job's package list), clippy on touched crates, and `git diff --check`.
