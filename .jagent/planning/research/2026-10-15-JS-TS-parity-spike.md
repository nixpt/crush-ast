# JS/TS parity spike — baseline, design and dispatch plan

Status: **design, ready to dispatch. Parser decided 2026-10-15 (option B).** vega, 2026-10-15. Baseline measured on main `5f223a9`
(after #184 Python parity, #187–#192). Tickets: **CRUSH-zorro-4 … CRUSH-zorro-23**.

## Goal and bar

Make JavaScript, and TypeScript through the same walker, as trustworthy as Python became in #184:

- **VM exact.** `crush` prints byte-for-byte what `node` prints for every corpus program.
- **AOT exact-or-loud.** The AOT Rust and C backends either match exactly or refuse at compile time.
- **Never silently wrong**, on any engine. A program crush can't run fails with a message that names
  the unsupported construct, never a generic "load from uninitialised slot".

The reference is real `node` (v26 here). It also runs `.ts` directly through built-in type stripping,
so TypeScript needs no `tsc`: the TS reference is `node file.ts`. Like node, crush stays
parser-only: no type checking.

## Baseline (measured)

54 probe programs, one feature each, run through `js_to_cast(src, "js")` → frontend → CVM1 and
compared with `node -e`. The probe is reproduced as the seed corpus of CRUSH-zorro-4.

**15 match · 33 loud · 6 silently wrong.**

| area | match | loud (current error) | **silently wrong** |
|---|---|---|---|
| variables & scope | `let/const/var`, `var` hoisting | — | **bare block `{…}` dropped** (z-5) |
| functions | decl, hoisted call, arrow, recursion | default params ("stack underflow"), rest params, closure capture & function values (CRUSH-251) | — |
| control flow | if/else, for, for-of, while+break, do-while, switch | for-in (follow-up `join`), try/catch (`new Error`) | **labels ignored** (z-6), **`finally` skipped on return** (z-7) |
| operators | `**`, `%`, template literals | `??`, `?.`, `typeof`, bitwise, `==`/NaN, `1/0`, int overflow | **`'x' + 6.0` → `x6.0`** (z-9) |
| strings | — | all methods ("unknown capability" / "uninitialised slot") | — |
| arrays | — | indexOf/includes/sort/slice/splice, map/filter/reduce | **`console.log([1,'a'])` format** (z-10) |
| objects | — | mutation + `Object.keys`, spread, destructuring | **`console.log({a:1})` format** (z-10) |
| builtins | — | Math.*, parseInt/Number/String/toFixed, JSON, Map/Set | — |
| classes | — | `class`, `extends`/`super` | — |
| async / generators | — | `async`/`.then`, `function*` | — |
| misc | — | `undefined` as a value, `console.log` with `undefined` | — |

TypeScript, measured separately:
- The `ts` dialect strips annotations, typed functions and `interface` correctly.
- `enum` is loud.
- **TS code under dialect `typescript` or `js` prints `null` (silently wrong; z-8).**

A bare block was also confirmed directly: `{ console.log(5); } console.log(6)` prints only `6`, and
`{ x = 2; }` never assigns.

**More silently-wrong behaviour found by reading `lower_swc.rs` and the compiler.** The probe missed
these, or hit a loud error first in the same program:
- `&&`/`||` produce booleans, not operand values: `0 || 'dflt'` → `true` (z-22).
- `==`/`!=` are lowered as `===`/`!==`: `'1' == 1` → `false` (z-22).
- Template literals use the raw text: `` `a\nb` `` prints a literal backslash-n (z-9).
- `for-in` over an array yields elements, not indices (z-16 / z-23).
- Object spread is dropped; every computed key `{[k]: v}` becomes `"[]"`; setter bodies are emptied (z-16 / z-23).
- Spread in array literals and call arguments loses its `...` (z-23).
- Class `constructor` dropped and `extends` ignored; `yield x` just evaluates `x` (z-23; implementation is out of scope).
- TS `enum`/`namespace` are dropped, so later uses fail as undefined names (z-21 / z-23).
- `console.error`/`console.warn` write to stdout (z-23).
- The walker registry parses `.ts` files as ES, because the source goes through `walker_N.tmp` (z-8).

So the true silently-wrong count is well above the 6 the probe found. CRUSH-zorro-23 turns all of these
loud first; the area tickets then make them correct.

## Root-cause clusters

1. **Silent drops in the lowering** (blocks, labels, finally, dialect fallback to a null program). These are the P0s.
   Each comes from a catch-all that returns nothing instead of failing. CRUSH-zorro-5 also turns the
   statement catch-all into a loud error, so new drops can't appear.
2. **JS value semantics differ from crush's.** `+` with strings, `/` (fixed in #189), division by zero,
   bitwise ToInt32, `undefined` vs `null`, and integer overflow (JS numbers are doubles). Approach (as in
   #189): add **`js.*` capabilities** with JS semantics, used only by the JS walker; crush's own operators are unchanged.
3. **Missing builtin/method surface.** Strings, arrays, Math, conversions, JSON, Map/Set. Mostly mapping to
   existing stdlib capabilities, but each is a hand-written arm in `lower_swc.rs` today → CRUSH-zorro-11
   makes it a table.
4. **Blocked on the object model / function values.** Closures and higher-order methods depend on
   CRUSH-251 steps 2–4 (design #191, step 1 #192 merged). Classes, `this` and prototypes depend on later
   CRUSH-251 slices.
5. **Error messages.** About a third of the loud failures say "load from uninitialised slot N", which tells
   the user nothing. Each area ticket owns replacing them with "`X` is not supported by the JS walker yet".

## Parser choice: swc vs boa, with bozo in the picture

### What bozo is, and what it plans for JS

**bozo** (`nixpt/bozo`) is the script layer for exosphere's browsers (surfer, voyager). Page scripts are
walked by crush-ast's walkers into CAST and run on crush (VM/FastVM/AOT); surfer's `bozo_bridge.rs` already
implements its `DomBridge`. surfer runs page JS on `boa_engine` 0.22 today ("Path C"); moving to bozo
("Path A", SURF-BOZO-1) is how it drops the interpreter. bozo's M7 plan (all backlog) overlaps this spike
almost entirely:

| bozo ticket | plans | overlaps here |
|---|---|---|
| BOZO-M7-1 `bozo-frontend` | a **second JS frontend**: `boa_parser` 0.22 (no swc, no `boa_engine`), its own scope/semantic pass (bindings, closure capture, `this`, hoisting, TDZ), classes desugared to prototypes, CASM emitted directly; TS via swc behind an optional `ts` feature, never in the browser's default build | z-5..9, z-11, z-16, z-18, z-20, z-21, z-22 |
| BOZO-M7-2 JS runtime library | ToPrimitive/ToNumber/ToString, `==`, `typeof`, Object/Array/String/Number/Math/JSON/Map/Set/Error builtins | z-10, z-12, z-14, z-15, z-17, z-19, z-22 |
| BOZO-M7-5 conformance gate | test262 subset + differential vs Boa (oracle) + surfer's page probe | z-4 (node as the oracle) |
| CRUSH-250 (crush-ast) | `boa_parser` 0.22 as the default for `.js`, swc only for TS, typed `Unsupported` errors | the parser decision itself |

As planned, two JS frontends would do the same semantic work: crush-lang-js's walker (this spike) and
bozo-frontend. That contradicts bozo's own rule ("fix upstream in crush-ast by default; bozo stays thin").

### Measured (2026-10-15, `cargo tree -e normal`, unique crates)

| what | crates | new crates for surfer (which already has `boa_engine` 0.22) |
|---|---|---|
| swc parser (`swc_ecma_parser` 41 + ast/common/atoms, crush-lang-js today) | 88 | **36** |
| `boa_parser` 0.22 (+ ast/interner) | 60 | **1** |
| `boa_engine` 0.22 (surfer today) | 130 | — |
| `swc_fast_ts_strip` 23 (node-style TS stripper; last release 2025-06) | 146 | 81 |

After surfer drops `boa_engine` (its goal), its JS parser costs 60 crates with boa versus 88 with swc.

### Other facts

- **TypeScript:** only swc parses TS; boa has no TS parser. The cheap way to keep a *single* lowering is
  to use the swc parser (already a dependency) only to find the type-annotation spans and blank them,
  which preserves positions (node's strip-only approach), then feed the result to the boa lowering. That
  costs 88 crates, only when the `ts` feature is on. `swc_fast_ts_strip` does the same job but is heavier
  and stale.
- **Lowering maturity today:**
  - `lower_swc.rs` is 1,660 LOC and passes the 29-program node corpus.
  - `lower_boa.rs` is 955 LOC on boa **0.21**, feature-gated off, has no plain `=` assignment
    lowering, turns classes into an empty `StructDef` and async into `null`; the node corpus is disabled
    under it.
- **Most of this spike's cost is parser-independent:**
  - the scope/closure pass and the CRUSH-251 primitives;
  - the JS runtime library (`js.*` capabilities: ToString, `==`, inspect, string/array/Math/JSON builtins);
  - the harness and its corpus.
  The swc lowering is name-based (methods lose their receiver, `new X` is a call to `"new X"`), so the object
  and closure waves are close to a rewrite in either parser.

### Options

- **A. Keep swc for JS and TS (the current plan).** The least work in crush-ast now, and the existing
  lowering keeps improving. But surfer/bozo pay +36 crates, or 88 instead of 60 after dropping
  `boa_engine`. SURF-BUILD-1 and CRUSH-250 say swc must not be in the browser build, so bozo would still
  build its own boa frontend, and the two frontends diverge.
- **B. One JS frontend in crush-ast, on `boa_parser` 0.22 (recommended).**
  - The semantic-pass lowering of BOZO-M7-1 lives **upstream in crush-lang-js**, as bozo's rule asks;
    bozo consumes it.
  - The runtime library of BOZO-M7-2 is crush-ast `js.*` capabilities, shared by the generic walker,
    crush-aotc and the browser.
  - TS uses swc only to blank type spans, behind a `ts` feature, never in the browser default.
  - One parser version across crush-ast, bozo and surfer.
  - The harness (z-4, node oracle) is the shared gate. bozo's M7-5 adds test262 and Boa-differential runs
    on top.

  Cost: CRUSH-250 becomes wave 2 (replacing z-11). The new lowering must reach the swc lowering's corpus
  results before swc is retired for `.js`, and small fixes landed in `lower_swc.rs` meanwhile are throwaway.

### What changes in this plan under B

- **Unchanged, parser-independent:**
  - z-4 (harness);
  - z-13 (number model);
  - z-23 (make silent constructs loud, cheap and stops wrong output now);
  - the `js.*` runtime capabilities inside z-9, z-10, z-12, z-14, z-15, z-17, z-19, z-22.
  These are the durable assets: the corpus programs and the runtime capabilities.
- **Retargeted to the new boa lowering, which owns them as acceptance tests:** z-5, z-6, z-7, z-16, z-18,
  z-20 and the lowering half of z-9/z-22. Fix them in `lower_swc.rs` only if swc stays the `.js` path for
  long.
- **Replaced:**
  - z-11 (the split of `lower_swc.rs`) is replaced by CRUSH-250 widened into "boa 0.22 lowering with a
    scope/semantic pass", absorbing BOZO-M7-1's design.
  - z-8 shrinks to the TS-preprocessor path.
- **Coordinate with bozo:** BOZO-M7-1 and M7-2 move upstream into crush-ast, or are closed as superseded
  here, and bozo keeps the browser-specific parts (DomBridge, sandbox, XIP, the event loop of M7-3).

## Decisions

- **DECIDED 2026-10-15 (owner): option B.** One JS frontend in crush-ast on `boa_parser` 0.22, shared with
  bozo; swc only to strip TS types. CRUSH-250 is widened into that frontend (wave 2) and CRUSH-zorro-11 is
  superseded. The tickets are retargeted as listed under *What changes in this plan under B*.
  *Original framing, kept for context:* **Parser for `.js`: option A (keep swc) or option B (one JS frontend on `boa_parser` 0.22 in crush-ast,
  shared with bozo; swc only for TS).** See *Parser choice* above. **Recommendation: B.** It's what the
  browser needs (+1 crate for surfer instead of +36, and no swc in the default build), and it avoids two JS
  frontends (crush-lang-js and bozo-frontend) doing the same semantic work. Most of this spike's work is
  parser-independent either way. Decide before wave 2. Under B, wave 2 becomes CRUSH-250 (widened) instead
  of z-11.

- **DECIDED 2026-10-15 (owner): number model (CRUSH-zorro-13) = option (a)**, JS arithmetic promotes Int to
  Float on overflow. Details in the ticket. *Original framing, kept for context:* **Number model (CRUSH-zorro-13).** JS numbers are doubles; crush has `Int(i64)` + `Float(f64)` and errors on
  overflow (`2**53 + 1`, `2**64` fail today). Options:
  - (a) JS arithmetic promotes to Float on overflow. **Recommended:** ints stay ints for indexing and loops,
    and output matches node for all safe integers.
  - (b) every JS number is a Float.
  - (c) leave overflow loud.

  With this, nothing in the plan is waiting on a decision.

## Tickets and dispatch plan

| wave | ticket | what | pri | depends on |
|---|---|---|---|---|
| 0 | **CRUSH-zorro-4** | harness: feature corpus vs node, known-loud allow-list, SILENTLY-WRONG fails, AOT `js_parity.rs`, TS twin, freshness | P1 | — |
| 1 | CRUSH-zorro-23 | make every silently mis-lowered construct fail loudly on the swc path (incl. bare blocks, labels, `finally`) | **P0** | — |
| 1 | CRUSH-zorro-9 | `+`/template literals use JS ToString (`js.add`; durable runtime capability) | **P0** | — |
| 1 | CRUSH-zorro-22 | `&&`/`||` return operand values; `==` coerces (durable runtime capabilities) | **P0** | — |
| 1 | CRUSH-zorro-10 | `console.log` of arrays/objects like node (`js.inspect`) | P1 | z-9 |
| 2 | **CRUSH-250** (widened) | **the JS frontend:** `boa_parser` 0.22 lowering + scope/semantic pass + builtin table; flips `.js` once ≥ swc on the harness | P1 | z-4, CRUSH-251 step 2 |
| 2 | CRUSH-zorro-8 | TS: swc parser blanks types → boa lowering (`ts` feature); loud on bad extensions | P1 | CRUSH-250 |
| 3 | CRUSH-zorro-5 / 6 / 7 | bare blocks + scoping / labeled loops / `finally`, in the new lowering | P1 | CRUSH-250 |
| — | ~~CRUSH-zorro-11~~ | superseded by CRUSH-250 | — | — |
| 3 | CRUSH-zorro-12 | values & operators (`undefined`, `typeof`, `??`, `?.`, bitwise, Infinity/NaN) | P1 | CRUSH-250 (runtime part can start earlier) |
| 3 | CRUSH-zorro-13 | number model (owner decision first) | P1 | decision |
| 3 | CRUSH-zorro-14 | string methods | P1 | CRUSH-250 (runtime part can start earlier) |
| 3 | CRUSH-zorro-15 | array methods (non-callback; `sort()` is string order) | P1 | CRUSH-250 (runtime part can start earlier) |
| 3 | CRUSH-zorro-16 | objects: mutation, `Object.*`, spread, destructuring, for-in | P1 | CRUSH-250 |
| 3 | CRUSH-zorro-17 | builtins: Math, conversions, JSON, Map/Set | P1 | CRUSH-250 (runtime part can start earlier) |
| 3 | CRUSH-zorro-18 | default and rest parameters | P1 | CRUSH-250 |
| 3 | CRUSH-zorro-19 | errors: throw / `new Error` / `e.message` / catch binding | P1 | CRUSH-250 (runtime part can start earlier) |
| 4 | CRUSH-zorro-20 | callbacks + closures (map/filter/reduce/sort(cmp)/closures) | P1 | CRUSH-250, CRUSH-251 step 3, z-15 |
| 4 | CRUSH-zorro-21 | TS: erase type-only constructs, enums, loud namespaces/decorators | P2 | z-8, CRUSH-250 (enums need z-16) |

How to hand it out:
- **Wave 0** first, one agent: every later ticket's done-condition is "my area's corpus programs leave the allow-list".
- **Wave 1** in parallel:
  - z-23 makes the swc path stop being silently wrong (blocks, labels, `finally` and the rest fail loudly);
  - z-9, z-22 and z-10 build durable `js.*` runtime capabilities.
- **Wave 2:** CRUSH-250, the new frontend. It's the critical path, best done by one agent in the sub-PR
  order in its ticket. z-8 (TS) follows once its lowering exists.
- **Wave 3** in parallel. The **runtime-library halves** of z-12/14/15/17/19 can start during wave 2
  (capabilities in crush-vm, unit-tested); their wiring and z-5/6/7/16/18 go into the new lowering.
- **Wave 4:** closures (z-20) once CRUSH-251 step 3 lands, and TS constructs (z-21).

Every ticket carries a `## Dispatch` section with the bar, the worktree rule (worktrees go under
`crush-ast/.jagent/worktrees/`, next to the `buckets` symlink) and the gates.

## Existing tickets this spike touches

- **CRUSH-199** (JS walker silent drops; marked Backlog but partly stale, since switch, `/` and `push` were
  fixed in #180/#189). Its `finally` and top-level-block halves are superseded by z-7 and z-5. Close it
  or narrow it to the `compile_cast` semantic-pass issue that remains.
- **CRUSH-238** (walkers stub unsupported constructs as null). z-23 is its JS-walker slice.
- **CRUSH-250** (boa as the default for `.js`). See the decisions above.
- **CRUSH-251** (closures and object model). z-20 is blocked on its step 4; classes wait for its
  later slices. Step 1 landed in #192, but the ticket's status wasn't updated.
- **CRUSH-260** (`.length` and the JS number policy). `.length` is done; the number policy is z-13.
- **CRUSH-261** (JS→AOT vs node benchmark). Complementary: performance, not semantics.
- `.jagent/knowledge/architecture/language_frontend_matrix.md` marks JS "✅ Complete". Update it to point
  at this spike's harness numbers once z-4 lands.

Also outside this spike: `crush run` doesn't accept `.js`/`.ts` (only `.crush`/`.casm`/`.cvm1`). JS
is reachable through the walker CLI, `crush-aotc` and `crush-walk-run`. Worth a small follow-up once
parity is meaningful.

## Measuring progress

The harness prints per-area counts (match / loud / silently-wrong) on every run. The spike is **done**
when, for every area except classes, async and generators:
- the VM column is all match;
- the AOT columns are match or loud;
- the silently-wrong column is zero on every engine;
- the TS twin passes wherever its JS counterpart passes.

## Out of scope for this spike (follow-ups, not ticketed here)

- **Classes, `this`, `new`, prototypes, getters/setters, `instanceof`.** CRUSH-251 object-model slices after closures.
- **`async`/`await`/Promises, generators.** Needs a mapping onto crush tasks (spawn/await, #188's
  task-local HALT); a design of its own.
- **Modules** (`import`/`export`/`require`), `RegExp`, `Date`, `Intl`, BigInt.
- **AOT for areas that need new runtime machinery.** Loud is acceptable under the bar.
