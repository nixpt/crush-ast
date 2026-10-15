# JS/TS parity spike — baseline, design and dispatch plan

Status: **design, ready to dispatch**. vega, 2026-10-15. Baseline measured on main `5f223a9`
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

## Decisions needed from the owner

- **Parser for `.js`: swc or boa (CRUSH-250).** Today swc is used everywhere (boa is feature-gated and
  off). CRUSH-250 (P1, backlog) plans boa 0.22 as the default for `.js`, with swc kept for TS. All the
  lowering work here targets `lower_swc.rs`; if `.js` moves to boa, JS parity has to be redone on
  the boa lowering. **Recommendation:** keep swc for both JS and TS during this spike (one lowering,
  and TS needs swc anyway), and revisit CRUSH-250 against the parity harness afterwards. Decide before
  wave 2 (CRUSH-zorro-11).

- **Number model (CRUSH-zorro-13).** JS numbers are doubles; crush has `Int(i64)` + `Float(f64)` and errors on
  overflow (`2**53 + 1`, `2**64` fail today). Options:
  - (a) JS arithmetic promotes to Float on overflow. **Recommended:** ints stay ints for indexing and loops,
    and output matches node for all safe integers.
  - (b) every JS number is a Float.
  - (c) leave overflow loud.

  Nothing else in this plan needs a decision.

## Tickets and dispatch plan

| wave | ticket | what | pri | depends on |
|---|---|---|---|---|
| 0 | **CRUSH-zorro-4** | harness: feature corpus vs node, known-loud allow-list, SILENTLY-WRONG fails, AOT `js_parity.rs`, TS twin, freshness | P1 | — |
| 1 | CRUSH-zorro-5 | bare blocks dropped (+ loud statement catch-all) | **P0** | — |
| 1 | CRUSH-zorro-6 | labeled break/continue | **P0** | — |
| 1 | CRUSH-zorro-7 | `finally` on return/break/continue/throw | **P0** | — |
| 1 | CRUSH-zorro-8 | dialect handling; TS-in-JS prints null | **P0** | — |
| 1 | CRUSH-zorro-9 | `+`/template literals use JS ToString (`js.add`) | **P0** | — |
| 1 | CRUSH-zorro-22 | `&&`/`||` return operand values; `==` coerces | **P0** | — |
| 1 | CRUSH-zorro-23 | make every silently mis-lowered construct fail loudly | **P0** | — |
| 1 | CRUSH-zorro-10 | `console.log` of arrays/objects like node | P1 | z-9 |
| 2 | **CRUSH-zorro-11** | split `lower_swc.rs` by area + builtin/method registry (no behaviour change) | P1 | z-4 |
| 3 | CRUSH-zorro-12 | values & operators (`undefined`, `typeof`, `??`, `?.`, `==`, bitwise, Infinity/NaN) | P1 | z-11 |
| 3 | CRUSH-zorro-13 | number model (owner decision first) | P1 | decision |
| 3 | CRUSH-zorro-14 | string methods | P1 | z-11 |
| 3 | CRUSH-zorro-15 | array methods (non-callback; `sort()` is string order) | P1 | z-11 |
| 3 | CRUSH-zorro-16 | objects: mutation, `Object.*`, spread, destructuring, for-in | P1 | z-11 |
| 3 | CRUSH-zorro-17 | builtins: Math, conversions, JSON, Map/Set | P1 | z-11 |
| 3 | CRUSH-zorro-18 | default and rest parameters | P1 | z-11 |
| 3 | CRUSH-zorro-19 | errors: throw / `new Error` / `e.message` / catch binding | P1 | z-11 |
| 4 | CRUSH-zorro-20 | callbacks + closures (map/filter/reduce/sort(cmp)/closures) | P1 | **CRUSH-251 step 4**, z-15 |
| 4 | CRUSH-zorro-21 | TS: erase type-only constructs, enums, loud namespaces/decorators | P2 | z-4, z-8 (enums need z-16) |

How to hand it out:
- **Wave 0** first, one agent: every later ticket's done-condition is "my area's corpus programs leave the allow-list".
- **Wave 1** in parallel: seven small P0s touching different lowering arms. z-23 has the most leverage: it turns the unknown silent bugs into allow-listed loud ones.
- **Wave 2** alone. A mechanical split plus a registry, landed fast.
- **Wave 3** in parallel (up to 7 agents). After z-11, most of it is registry rows, so conflicts stay small.
- **Wave 4** when CRUSH-251 step 4 lands.

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
