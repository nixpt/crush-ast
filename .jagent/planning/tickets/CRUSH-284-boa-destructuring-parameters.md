# CRUSH-284 — boa walker: destructuring parameters

| Field | Value |
|-------|-------|
| **ID** | CRUSH-284 |
| **Priority** | P1 |
| **Status** | Open |
| **Phase** | COSM K3 follow-up (boa walker) |
| **Assignee** | muse |
| **Filed by** | muse — 2026-10-20, from the maya-engine sweep (CRUSH-276) |

## Problem

The boa walker refuses every destructuring parameter
(`crates/crush-lang-js/src/lower/stmt.rs`, `function_in_scope`:
"a destructuring parameter"). Maya's engine code uses the
options-object idiom pervasively:

```js
class Engine {
  constructor({ canvas = null, width = 960, height = 540 } = {}) { … }
}
```

Four maya files (Engine/Effects/Loop/Physics) hit this as their
first blocker; it is also the common JS shape for options bags
generally.

## Scope

Parameters only (functions, methods, constructors, statics,
arrows where parameters are otherwise allowed):

- object patterns: `{a}`, renames `{a: b}`, per-property defaults
  `{a = 1}`, a parameter-level default (`{…} = {}`), rest
  (`{...rest}`) if it falls out naturally;
- array patterns: `[x, y]`, holes, per-element defaults, nested
  patterns if cheap;
- the destructured parameter counts as **one** visible parameter
  for the CRUSH-zorro-18/CRUSH-280 signature machinery (a default
  at the parameter level makes it a defaulted parameter; property
  defaults do not).

Out of scope (stay loud): destructuring variable declarations and
assignments (separate shapes), destructuring `catch`/loop heads.

## Semantics to keep

- A property default applies when the property is absent
  (JS: `undefined`); crush's null stands in for it.
- Destructuring `null`/`undefined` itself throws in JS; the lowered
  property reads on a non-object fail loudly at run time, which is
  close enough — no silent wrong values.
- Defaults are evaluated in parameter scope, in order, as with
  plain default parameters.

## Evidence

- boa e2e: maya's `constructor({ width = 960, height = 540 } = {})`
  shape (omitted/partial/full options), renames with a default
  reading an earlier property, array patterns with holes and
  out-of-range defaults, nested patterns, `[a, b] = […]` parameter
  defaults; object/array rest and assignment targets stay loud.
- Node corpus: `class_destructuring` matches node on boa (loud on
  swc as out-of-scope classes, and on AOT as zorro-43).
- Maya sweep (whole tree, 260 files): 15 walk; no file's first
  blocker is a destructuring parameter any more. Engine/Effects/
  Loop/Physics advanced to cross-module `new` (CRUSH-110),
  member `new a.B()`, `performance.now()`, and `new Map`
  (zorro-17) respectively. One file (games/decor.js) now reaches
  destructuring *declarations*, which stay out of scope.
