# CRUSH-285 — VM/walker: class temporal dead zone is not enforced at run time

| Field | Value |
|-------|-------|
| **ID** | CRUSH-285 |
| **Priority** | P3 |
| **Status** | Open |
| **Phase** | COSM K3 follow-up (boa walker) |
| **Assignee** | unassigned |
| **Filed by** | muse — 2026-10-21, residual of CRUSH-281 |

## Problem

CRUSH-281 pre-registers module classes so function bodies may
construct classes declared later in the same statement list. The
walker refuses a statement-position `new C()` before `C`'s class
declaration (the temporal dead zone), but the run time cannot:
the VM's kind table is static, so a function that is *called
during* the list's own evaluation, before the class item has run,
constructs the class where JS would throw a ReferenceError:

```js
function make() { return new Counter(1); }
console.log(make());        // JS: ReferenceError (TDZ)
class Counter { constructor(x) { this.x = x; } }
```

This lowers and runs today. The construction itself is correct —
only the error JS would raise is missing — so nothing is silently
miscomputed; the gap is a missing refusal.

## Suggested shape

Runtime initialisation state for kinds: a kind starts
uninitialised, the class declaration's position in `main` flips it
(class declarations currently emit no statements — the compiler
could emit a marker there), and `Construct`/`LOAD_KIND` refuse an
uninitialised kind. Needs a design pass over how the marker is
represented in CASM (a new op vs. a synthetic call) and what it
means for non-JS kind sources.
