# CRUSH-281 — boa walker: `new C()` inside a top-level `function` can't see module classes

| Field | Value |
|-------|-------|
| **ID** | CRUSH-281 |
| **Priority** | P2 |
| **Status** | Done — PR #289 merged 2026-10-22 |
| **Phase** | COSM K3 follow-up (boa class slice) |
| **Assignee** | muse |
| **Filed by** | muse — 2026-10-20, found while testing CRUSH-285 review #5 |

## Problem

```js
class C { constructor(x) { this.x = x; } }
function f() { return new C(1); }
console.log(f());
```

fails while lowering: "`new C`: only a class declared in this module
constructs yet". The script-level lowering
(`crates/crush-lang-js/src/lower/stmt.rs`, the script `items` path)
lowers *all* top-level `function` declarations first
(`hoisted_functions(.., Hoist::Script)`) and only then the items in
order, so no class is registered when a top-level function body is
walked — regardless of declaration order. The same body inside a
method, or in a top-level statement, works.

This is legal JS (the class is initialised before `f` is *called*;
only a call during module evaluation, before the class declaration
runs, would hit the TDZ). Maya's engine code wraps most work in
top-level functions, so this will gate files once destructuring
parameters (their current first blocker) land.

## Suggested shape

Pre-register top-level class *names* (the resolved kind names) in a
pass over the script items before `hoisted_functions`, so `is_kind`
answers for the whole module; `new C()` then lowers to a `Construct`
whose kind the class declaration fills in when its item is reached.
Statement-order `new` before the class declaration would also start
lowering (a TDZ the walker would no longer catch — the constructor
call itself fails loudly at run time only if the class item hasn't
run; weigh whether to keep statement-order strictness).

## Resolution

Statement lists (the module top level, and every function body)
pre-register their classes — kind names, parents and member
signatures (`Lower::preregister_kinds`, with the signature
registration factored out of `class_kind`) — before any function in
the list is lowered. `new C()` and `C.f()` may use a
pre-registered class from code that runs after the list has: a
function body (`fn_depth > 0`) or a field initialiser
(`deferred_new`). A statement-position `new` before the class item
still refuses, naming the temporal dead zone. Class expressions
bound by a declaration and exported classes pre-register too.

Residual: a function *called during* the list's own evaluation,
before the class item has run, constructs the class where JS would
throw a ReferenceError — the VM's kind table is static, so the TDZ
is not enforced at run time. Filed as CRUSH-291; the construction
itself is correct, only the error is missing.

## Review fix (kai, PR #289)

The first cut keyed pre-registration by name alone, which leaked:
a statement of the declaring list itself constructed (its
`fn_depth > 0` inside any function body), a parameter or local
named like the class was constructed as the class, a
function-local class stayed visible to sibling functions, and a
class expression's inner name (`const C = class K { … }`) was
constructible outside the class. Now:

- pre-registration records the `fn_depth` of the statement list
  that owns it, and only a function nested *deeper* than that list
  (or a field initialiser) may construct a pending class;
- visible classes and class bindings are snapshotted and restored
  around every function body and block, so registrations never
  leak to siblings or out of blocks;
- scopes tag class bindings, and `new`/`C.f()`/calling `C()` go
  through that tag (`Lower::class_ref`): a shadowing binding is
  refused as not-the-class, and a name that is no class binding
  (the `K` above) does not construct;
- two classes resolving to one kind name refuse loudly (scoped
  kind names are the real fix, future work).

Covered by `test_boa_new_pending_class_scope_rules`.

## Survey 2026-10-31 (status)

PR #289 (`agent/muse/CRUSH-281`) merged 2026-10-22. Previous status: `Done (PR pending)`.
