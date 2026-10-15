# CRUSH-250 — the JS frontend: `boa_parser` 0.22 lowering with a scope/semantic pass (default for `.js`; shared with bozo)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-250 |
| **Priority** | P1 — wave 2 of the JS/TS parity spike; first step of the browser's "drop boa_engine" path |
| **Status** | In Progress |
| **Phase** | M6 |
| **Assignee** | vega |
| **Dependencies** | CRUSH-zorro-4 (parity harness); CRUSH-251 step 2 (function values, #194) for closures; later CRUSH-251 slices for the object model |
| **Estimated effort** | L (split into the sub-PRs below) |

> **Widened 2026-10-15 by the owner's parser decision (option B).** See the "Parser choice" section of
> `research/2026-10-15-JS-TS-parity-spike.md`. JavaScript gets **one** frontend, in crush-ast, on
> `boa_parser` 0.22. It is shared by the generic walker, crush-aotc and the browser (bozo/surfer), and
> it absorbs the design of bozo's planned `bozo-frontend` (BOZO-M7-1), so bozo stays thin. swc remains
> only to strip TypeScript types (CRUSH-zorro-8). The original problem statement is kept below.

## Problem (original)

surfer-browser wants page JS on the crush path (bozo frontend → CASM → CVM1/FastVM/JIT/AOT) so it can
drop `boa_engine` (≈153k LOC, 67 crates of which 43 nothing else in surfer uses) and keep only Boa's
**parser** (`boa_parser`+`boa_ast`+`boa_interner`, ≈54k LOC, 65 crates).
- crush-lang-js already has a Boa backend, but it is pinned to **0.21**
  (`crates/crush-lang-js/Cargo.toml:29-31`) while surfer uses **0.22**, and it is off by default
  (`default = ["swc-backend"]`).
- Its lowering is thinner than swc's (`lower_boa.rs` 930 LOC vs `lower_swc.rs` 1521): classes become an
  empty `StructDef` at `lower_boa.rs:364-370`, and async/generator expressions become `null` at `:758-762`.
- With swc as the JS path, surfer also carries swc's 147-crate closure through `crush-lang-sdk`'s default
  `polyglot-javascript` feature.

## Problem (added 2026-10-15)

- Both existing lowerings are **name-based**: methods lose their receiver, `new X` is a call to `"new X"`,
  there's no scope analysis, and several constructs are silently dropped (see the spike's baseline:
  15 match / 33 loud / 6 silently wrong, plus more found in a code read).
- bozo planned to fix this in a second frontend of its own (BOZO-M7-1); this ticket does it once, upstream.
- Measured cost for surfer, which already has `boa_engine` 0.22:
  - `boa_parser` adds **1** new crate; the swc parser adds **36**.
  - Once surfer drops `boa_engine`: boa is 60 crates, swc 88.

## Success criteria

- [ ] **Versions/features:**
  - `boa_parser`/`boa_ast`/`boa_interner` at **0.22**, one Boa version across the workspace, bozo and surfer.
  - A `js-boa` build has **no `swc_*`** in `cargo tree -e normal`.
  - swc sits only behind an optional `ts` feature (CRUSH-zorro-8).
  - `crush-lang-sdk`'s `polyglot-javascript` uses boa; there's a separate `polyglot-typescript`.
- [ ] **A real front end, not a name-based lowering.** A scope/semantic pass over the Boa AST: bindings with
  identity (alpha-renamed, since crush locals are name-keyed per function); `let`/`const` block scope with
  TDZ; `var` and function hoisting; per-iteration `let` in `for`; closure-capture analysis feeding
  CRUSH-251 (Func/MAKE_CLOSURE/CALL_VALUE, and capture cells in step 3); `this` per function kind.
- [ ] **A data-driven builtin/method table** (moved here from CRUSH-zorro-11), mapping JS globals and
  receiver methods to the `js.*` runtime-library capabilities (CRUSH-zorro-9/10/12/14/15/17/19/22). Area
  tickets add rows, not match arms.
- [ ] **Unsupported is typed and loud:** `Unsupported { construct, span }`, never a silent `null`/drop.
  Every catch-all arm bails.
- [ ] **Switch-over gate.** The parity harness (CRUSH-zorro-4) runs both lowerings during the migration. `.js`
  defaults to boa only when boa is **≥ swc in every area with zero silently-wrong results**. After that,
  `lower_swc.rs` is deleted for JS (TS goes through the strip-to-boa path of CRUSH-zorro-8).
- [ ] **Constructs owned by this lowering**, as corpus programs. Several are their own tickets, retargeted
  here: bare blocks (z-5), labeled break/continue (z-6), `finally` (z-7), objects/destructuring/spread/for-in
  (z-16), default/rest params (z-18), closures and higher-order calls (z-20).

## Sub-PRs (suggested)

1. Bump to 0.22 + feature layout (`js-boa`, `ts`); no behaviour change on the swc default.
   **Done in sub-PR 1:** boa 0.22; features `swc-backend` (the `.js` lowering until step 6), `ts`
   (implies it), `js-boa`; `.js` stays on swc while `swc-backend` is on, so turning `js-boa` on via
   unification changes nothing; `freevars` has a boa twin (kept in step with swc by a test) and is what
   crush-lang-sdk's `polyglot-javascript` now uses (the feature keeps `swc-backend` until step 6, so
   `.js` walked through the SDK stays on swc; step 6 drops it and swc leaves the SDK's default tree);
   `polyglot-typescript` carries `ts`. `JsLowering`/`JsFrontend::with_lowering` pick a lowering
   explicitly.
2. Scope/semantic pass + statement/expression lowering to parity with `lower_swc.rs` on the harness.
   **Done in sub-PR 2:**
   - `lower/` (`mod.rs`, `scope.rs`, `stmt.rs`, `expr.rs`) replaces `lower_boa.rs`.
     - Scope pass: function-level bindings keep their names (hoisted on entry); block bindings get fresh
       names, so shadowing is sound.
     - A nested `function` is a hoisted closure; `var x;` is hoisted (no reset on re-run).
     - Typed `Unsupported { construct, at }` with line/column, and no placeholder `null`s. Classes, `new`
       and `this` stay loud until COSM's JS walker slice.
     - Integer literals past `i64` lower to `Float` (CRUSH-zorro-13 (a)).
   - `js-boa` is a default feature, so the harness runs and **enforces** boa on every build (`BOA_FIXED`
     on the VM, `BOA_EXACT`/`BOA_LOUD` on AOT) and prints the switch-over gate.
   - Results:
     - VM: boa 68/40/0 vs swc 63/45/0.
     - AOT Rust/C: boa 61/44/3 vs swc 59/44/5; boa's 3 silent are swc's known CRUSH-zorro-12 ones.
   - swc fixes found by the new corpus programs:
     - `var x;` no longer resets `x` (it was silently wrong);
     - a nested `function` declaration is refused (it was silently wrong on AOT).
   - Not done yet: TDZ (a read of `let`/`const` before its declaration).
3. Builtin/method table wired to the `js.*` runtime library.
   **Done in sub-PR 3:**
   - `lower/builtins.rs` has three tables:
     - `GLOBALS`: global and namespace functions;
     - `METHODS`: receiver methods;
     - `VALUES`: global names read as values.
   - Each row has an arity and a lowering: a capability, the `console.log` line, a receiver-dispatched
     method, or `Planned(ticket)`.
   - A call or read that is neither a user binding nor a row fails at walk time, naming it (and its
     ticket). It never reaches "Undefined function", "unknown capability" or "uninitialised slot".
   - A top-level function reading a script variable is refused (AOT read it as `null`).
   - Area tickets turn `Planned` rows into lowerings.
4. Closures/function values (CRUSH-251 steps 2–3).
   **Done in sub-PR 4:**
   - A top-level `function` that uses script variables is a closure in `main` over them (CRUSH-251
     cells); one that doesn't stays a crush function of its own.
   - In a scope with hoisted closures, the scope's `let`/`const` bindings are created on entry (a fresh
     one per block entry) and their declarations assign. A closure hoisted ahead of a `let` used to see
     a stale cell, a silent `null` that only boa had.
   - New corpus programs: `fn_script_vars`, `fn_script_late`, `scope_hoist_late_let`,
     `scope_hoist_block_loop`.
   - swc reads script variables as `null` on AOT (pre-existing): CRUSH-zorro-54.
   - Not done:
     - calling a value that isn't a name (`f()()`, an IIFE, `arr[i]()`) needs a call-on-value CAST
       expression (compiler change);
     - closures on AOT (the backends refuse `make_closure`/cells).
5. The constructs listed above (one PR each, or as their tickets).
6. Flip the `.js` default once the gate passes; retire the swc lowering for JS; update
   `language_frontend_matrix.md`.
   **Gate items besides the harness (boa >= swc in every area, VM and AOT, 0 silent):**
   - **A top-level function that reads a script variable must run.** (VM: done in sub-PR 4; AOT waits
     for closures there.) It is common JS
     (`let total = 0; function add(n) { total += n; }`).
     - Today it is refused at walk time: top-level functions are their own crush functions, AOT read
       the variable as `null`, and the VM failed.
     - Intended lowering: script bindings that functions use become captured cells (CRUSH-251 ops),
       with the functions that use them closures in `main`; or the bindings become globals.
     - boa must not become the default while this is loud.
   - `#235`'s argument-order fix lands before `push` (or any method row) accepts more than one
     argument. (Done: #238 fixed the order. `push` takes any number of values: in statement position
     they are evaluated into hidden locals and pushed one by one, since the capability appends one value.
     As a value, `push` is `len` of the returned array; AOT's `push` now returns the array like the VM's.)
   - CRUSH-zorro-54 lands first: AOT reads an unassigned local as a run-time error, never `null`.

## Not this ticket

- Prototypes/classes/`new`/`this` objects: CRUSH-251's object-model slices. This lowering emits through
  them when they land and is loud until then.
- async/generators: a separate design, together with bozo's BOZO-M7-3 event loop.
- Browser-specific parts (DomBridge, sandbox, XIP, the event loop): they stay in bozo.

## Files to modify

- `crates/crush-lang-js/Cargo.toml`: versions, features
- `crates/crush-lang-js/src/{lib.rs,backend/boa.rs,lower_boa.rs → lower/*,analyzer_boa.rs}`, plus a scope pass
- `crates/crush-lang-sdk/Cargo.toml`: `polyglot-javascript` / `polyglot-typescript` features
- tests under `crates/crush-lang-js/tests/` and the harness (CRUSH-zorro-4)

## Cross-refs

- surfer-browser `SURF-BOZO-1` (page JS → bozo), `SURF-BUILD-1` (drop swc from surfer), `SURF-BOA-1`.
- bozo `BOZO-M7-1` (its frontend: superseded by this ticket), `BOZO-M7-2` (its runtime library → the `js.*`
  tickets here), `BOZO-M7-5` (its conformance gate, layered on CRUSH-zorro-4).
- The JS/TS parity spike: CRUSH-zorro-4..23.
