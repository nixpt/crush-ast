# CRUSH-287 — boa walker: destructuring declarations

| Field | Value |
|-------|-------|
| **ID** | CRUSH-287 |
| **Priority** | P1 |
| **Status** | Done — PR #292 merged 2026-10-29 (branch `agent/muse/CRUSH-286`, commit `e3308f8`) |
| **Phase** | COSM K3 follow-up (boa walker) |
| **Assignee** | muse |
| **Filed by** | muse — 2026-10-21, follow-on to CRUSH-284 |

## Problem

CRUSH-284 lowered destructuring *parameters*; destructuring
*declarations* still refuse ("a destructuring declaration"):

```js
const { width, height } = canvas;
const [first, , third = 40] = items;
let { x: renamed = 1 } = point;
```

This is the same pattern machinery over a declaration's
initializer, and it is common inside function bodies (maya:
games/decor.js reaches it as its first blocker, and the CRUSH-276
sweep counted destructuring patterns "more inside bodies" than at
the top level).

## Scope

- `var`/`let`/`const` declarations with object or array patterns,
  at any statement-list level (module, function body, block),
  including nested patterns and per-element defaults (absence
  semantics as in CRUSH-284: a present null keeps its null).
- The initializer is evaluated once (into a hidden temporary)
  before any binding is written, as in JS.
- A declaration without an initializer binds the pattern's names
  to null (JS: `undefined`).

Out of scope (stay loud): destructuring assignment
(`({a} = obj)`), destructuring loop heads
(`for (const {a} of xs)`), `catch` parameters, and object/array
rest elements (as in CRUSH-284).

## Evidence (2026-10-21)

- Implementation: `variable()` dispatches pattern bindings to
  `destructuring_decl()`, which evaluates the initializer once
  into a hidden `__dsrcN` temporary and runs CRUSH-284's
  `destructure()` in declaration mode (`bind_leaf`): plain
  `var_decl_stmt` per binding, or `__crush_assign__` for
  predeclared block bindings (mirroring identifier declarations).
  Branch-forgetting records destructured names like identifier
  assignments. No-initializer declarations bind each name to null.
- Tests: `test_boa_destructuring_declarations_run` (object/array
  patterns, renames, defaults on absence, nesting, initializer
  evaluated once — a method returning 42 increments a counter
  exactly once across two bindings, a present null keeps its
  null, block shadowing, method bodies), no-init binds null, and
  `test_boa_destructuring_declarations_loud` (assignment, loop
  heads, rest elements stay loud). The pre-existing node corpus
  program `obj_destructure` moves to BOA_FIXED and runs exactly
  on AOT (BOA_EXACT).
- Suites: standard + walkers + clippy (`-D clippy::never_loop`)
  green.
- Maya sweep (throwaway, 260 files): walked count unchanged at
  15/260, but games/decor.js's first blocker advances from the
  destructuring declaration to `new Set` (CRUSH-zorro-17 host
  constructors). No file's first blocker is a destructuring
  declaration anymore.

## Survey 2026-10-31 (status)

Commit `e3308f8` (CRUSH-287: destructuring declarations) merged via PR #292 on 2026-10-29. Previous status: `In PR (stacked on CRUSH-284)`.
