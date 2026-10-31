# CRUSH-280 — boa walker: default/rest parameters on methods, constructors, statics

| Field | Value |
|-------|-------|
| **ID** | CRUSH-280 |
| **Priority** | P1 (maya blocker: 9 engine files) |
| **Status** | Done — PR #286 merged 2026-10-21 |
| **Phase** | COSM K3 follow-up (boa class slice) |
| **Assignee** | muse |
| **Filed by** | muse — 2026-10-20, from the CRUSH-277 maya sweep |

## Problem

The boa walker refuses default and rest parameters on class members:

```js
class A { constructor(x = 1) { this.x = x; } }
class A { m(a = 2) { return a; } }
class A { m(...xs) { return xs; } }
class A { static s(a = 3) { return a; } }
```

all fail with "a default parameter value (`x = …`) outside a `function`
declaration is not supported" (verified 2026-10-20). `function`
declarations support both since CRUSH-zorro-18 (hidden `__has_<param>`
flag parameters appended after the real ones; call sites adapt their
arguments); members were excluded because a method's call sites —
`CALL_MEMBER`, `CONSTRUCT` — hit the VM's exact-arity checks, and the
walker didn't record member signatures to adapt against.

Maya census: this blocks 9 files (Engine, Layer, Transport, Effects,
GameObject, Loop, Physics, Scene, Spatial).

## Fix (walker only; the zorro-18 ABI carries the semantics)

Members lower like `function` declarations (`function_in_scope` with
argument adaptation allowed), and the walker records every lifted
member's signature in a new `fn_sigs` map (keyed by lifted fn name),
pre-registered for a whole class before any member body lowers so
self-calls (`this.m()`, `new C()`) resolve. Call sites the walker
knows adapt their arguments with the existing `adapt_args`:

- `new C(...)` — the constructor's signature;
- `super(...)`, `super.m(...)`, `C.static(...)` — the known target fn;
- `this.m(...)`, `c.m(...)` where `c` was bound by `new C()` —
  `instance_vars` grows into a var→kind map, and the signature is the
  nearest definition up the receiver kind's parent chain;
- `new C(...).m(...)` — same, off the construction's kind.

Unknown-receiver calls (CAST `MethodCall`, detached method values)
keep passing raw arguments. Two wrong shapes survived review
(CRUSH-286 review): positional flags mean a *count-matching* raw or
home-adapted call could run silently wrong (an override with a
different signature; a defaulted member reached through an unknown
receiver). The walker now records every member call it emits and,
once the whole module is lowered, refuses (a) an adapted call whose
receiver kind has a subclass override of the method with a different
signature, and (b) a raw call while any member of that name has
default/rest parameters. The tracked-kind map also forgets a binding
assigned anything but its `new`, and after any branch/loop assigning
it. The re-review narrowed the forgetting further: only a plain,
statement-level `=` proves a kind (compound/logical operators and
assignments nested in larger expressions forget); every binding
assigned anywhere in a loop statement is forgotten before the loop
is lowered (the back-edge); and a binding any nested function
assigns is poisoned for good, since calling that function can swap
it at any point. What remains genuinely unknown (detached method values, receivers
from outside the module) still fails the VM's exact-arity check —
loudly, never silently wrong.

Accessors are unaffected: a getter/setter whose parameter list grows
a hidden flag fails `check_accessor_arity` at load (loud), as before.

## Evidence

- Repro (above) refused on the base branch, lowers+runs after.
- boa e2e: constructor/method/static defaults and rest (incl. a
  default reading an earlier parameter, `this.m()` self-calls,
  `new C().m()`), `super()`/`super.m()` with defaults, derived
  default-constructor forwarding over a defaulted parent constructor.
- Node corpus: `class_defaults` matches node exactly on boa (loud on
  swc/AOT as out-of-scope/zorro-43, like the other class cases).
- Maya sweep: none of the 9 files is blocked by default parameters
  any more; each now reaches its next wall (destructuring parameters
  in Engine/Effects/Loop/Physics, `new Map` in Spatial, member `new`
  in GameObject/Scene, imported consts in Layer, `fetch` in
  Transport).

## Survey 2026-10-31 (status)

PR #286 (`agent/muse/CRUSH-280`) merged 2026-10-21. Previous status: `` Implemented on `agent/muse/CRUSH-280` — awaiting merge ``.
