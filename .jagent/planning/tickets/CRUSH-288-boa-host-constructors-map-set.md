# CRUSH-288 — boa walker: host constructors `new Map` / `new Set` (zorro-17 slice)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-288 |
| **Priority** | P1 |
| **Status** | In PR (stacked on CRUSH-287) |
| **Phase** | COSM K3 follow-up (boa walker) |
| **Assignee** | muse |
| **Filed by** | muse — 2026-10-21, follow-on to CRUSH-287 |

## Problem

`new Map()` / `new Set()` refuse ("only a class declared in this
module constructs yet"), and every Map/Set method is a planned
row (CRUSH-zorro-17). Maya uses them pervasively — Maps for
caches, registries and spatial indexes, Sets for membership —
mostly as class fields (`this.cache = new Map()` in a
constructor, then `this.cache.has(k)` in methods). Keys are
strings, numbers (ids) and class constructors
(`components.set(component.constructor, …)`), so crush's
string-keyed `Value::Map` cannot represent a JS Map: non-string
keys are rejected there, and a missing-key read is an error
where JS returns `undefined`.

## Design

Representation (walker-only; no new VM/compiler surface):

- A JS Map is a CAST array of `[key, value]` pair arrays; a JS
  Set is a CAST array of values. Arrays are shared references in
  the VM, so JS aliasing/mutation semantics hold.
- Operations lower to synthesized module helper functions
  (`__js_map_get` etc., hidden names, emitted once per module
  only when used), built from the same CAST the walker already
  emits (loops, `Index`, `__crush_setindex__`, `array.push` /
  `array.pop`, `len`). Key comparison uses the walker's `==`
  (loose equality): exact for the string/number/bool keys and
  identity values maya uses.
- A receiver's host kind is *proven*, never guessed, mirroring
  the class-instance tracking (CRUSH-277 D4): a binding proven
  by a declaration initializer or a plain top-level `=` whose
  right side is `new Map()` / `new Set(...)`, forgotten by the
  same rules that forget instance kinds; a class field proven
  when every assignment to `this.f` in the class hierarchy
  proves the same host kind, validated at end of module like
  `check_member_calls`. Unproven receivers keep today's loud
  planned-row refusals.

## Scope

- `new Map()`, `new Map(iterableOfPairs)`, `new Set()`,
  `new Set(iterable)` in any expression position. A user class
  or binding named `Map`/`Set` shadows the host constructor.
- On proven receivers (plain bindings, `this.field`, a direct
  `new Map()`/`new Set(...)` expression): Map `get` (absent key
  → null), `set` (returns the map), `has`, `delete` (returns
  bool), `clear`, `keys()`/`values()`/`entries()` (fresh
  arrays), and the `.size` read (`len`). Set `add` (returns the
  set), `has`, `delete`, `clear`, `.size`. `for (const v of set)`
  works via the array representation; `for (const pair of map)`
  yields pair arrays.
- Wrong-kind methods (`map.add`, `set.get`), method values
  (`const g = m.get`), subscripts on a proven Map/Set
  (`m[k]` — use `.get`/`.set`), and wrong arities stay loud.

Out of scope (stay loud): `forEach` and callback iteration
(zorro-20), `Map`/`Set` subclassing, `WeakMap`/`WeakSet`,
spreading a Map/Set, and host-kind tracking through parameters
or captures (the zorro-44 general problem, as for instances).

Known representation corners (documented, not silent in
practice): object keys compare by the VM's `==` rather than
reference identity, so two distinct-but-deeply-equal object
keys would merge; `NaN` as a key/value never matches itself;
`console.log` of a Map/Set prints its array representation.

## Evidence (2026-10-21)

- Implementation: `lower/host.rs` (representation, the fourteen
  synthesized helpers, proof tracking, `check_field_host`);
  interception in `expr.rs` (member calls, `.size` reads,
  subscript refusals, assignment proof/forget mirroring
  `instance_kinds`), `stmt.rs` (declaration proofs, boundary and
  control-statement save/restore), `class.rs` (`new` carve-out —
  a user class or binding named `Map`/`Set` shadows the host
  constructor — and field-definition proofs). No VM/compiler
  changes.
- Tests: `test_boa_host_map_set_run` (string/number keys,
  insertion-order operations, `keys`/`values`/`entries`, clear,
  Set dedup, `new Map(pairs)` / `new Set(array)`, for-of over a
  Set, maya-shaped class fields, chained `new Map().set().get()`,
  portable VM run) and `test_boa_host_map_set_loud` (parameter
  receivers, subscripts, wrong-kind methods, `forEach`, arity,
  reassignment forgetting, hierarchy field poisoning). The
  `builtin_map_set` corpus program moves to BOA_FIXED and runs
  exactly on AOT (BOA_EXACT). The pre-existing "host
  constructor stays loud" assertion now uses `new WeakMap()`.
- Suites: standard + walkers + clippy (`-D clippy::never_loop`)
  green.
- Maya sweep (throwaway, 260 files): walked count unchanged at
  15/260 (cross-module `new`, CRUSH-110, still dominates), but
  first blockers advance — games/decor.js: `new Set` → spread
  arguments; core/ActionMap.js: Map fields → `.some()` (zorro-20
  callbacks); core/Spatial.js: Map fields → `get(cell).set(...)`,
  which needs host-kind tracking through a Map's *values*
  (nested collections — a follow-up); engine/Registry.js reaches
  the destructuring loop variable (CRUSH-287's out-of-scope).

## Follow-ups noted while sweeping

- Host kind of a Map's *values* (`this.cells.get(cell).set(...)`)
  — nested collections (maya: Spatial).
- Field proof assumes construction has completed; a method
  called from a constructor before the field's assignment line
  would read the field as its pre-assignment value where the
  walker trusts the proof.
