# CRUSH-281 — boa walker: `new C()` inside a top-level `function` can't see module classes

| Field | Value |
|-------|-------|
| **ID** | CRUSH-281 |
| **Priority** | P2 |
| **Status** | Open |
| **Phase** | COSM K3 follow-up (boa class slice) |
| **Assignee** | unassigned |
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
