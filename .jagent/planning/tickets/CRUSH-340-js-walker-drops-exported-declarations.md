# CRUSH-340 — JS walker drops the bodies of exported declarations

> **Renumbered** 2026-10-31 (planning survey) from `CRUSH-266`, which collided with the older
> `CRUSH-266-rust-crush-interop.md` (filed 2026-10-15; this one 2026-10-20). Branch `agent/muse/CRUSH-266`,
> PR #283, and commit messages that say CRUSH-266 about JS exported declarations mean this ticket.

| Field | Value |
|-------|-------|
| **ID** | CRUSH-340 |
| **Priority** | P1 |
| **Status** | Done |
| **Phase** | M4 |
| **Assignee** | muse |
| **Dependencies** | relates to CRUSH-110 (module.load is a no-op), CRUSH-238 (walkers stub unsupported constructs), CRUSH-251 (JS object-model primitives) |
| **Estimated effort** | S |
| **Filed by** | muse — 2026-10-20 maya-engine compile audit |

## Problem

`crush-lang-js`'s SWC lowerer (`crates/crush-lang-js/src/lower_swc.rs`,
`lower_module_decl`, `ModuleDecl::ExportDecl`) lowers
`export function f() {}`, `export class C {}` and `export const x = <expr>`
to a bare `Statement::Export { name, value: Var(name) }` and **never emits
the declaration itself**. Exported functions never enter
`Program.functions`, exported classes never reach `lower_class_decl`, and
exported variable initialisers are discarded. The resulting CAST compiles
to CASM/CVM1 that loads an uninitialised slot at runtime.

Any real-world ES module — which exports essentially everything this way —
is hollowed out. Measured against `maya-engine` (`worlds-workspace/maya-engine`,
37 ESM files in `src/`, 4,643 LOC, `three@0.180.0`): all 37 walk to CAST,
35/37 nominally reach CVM1 (the other two fail later, on `^`), but 0/37
retain their exported definitions (42 `export function`, 34 `export class`,
16 `export const/let` across the tree), and every compiled `main` either
crashes with `load from uninitialised slot 0` (`scatter.js`, `labels.js`,
`biomes.js`, `Protocol.js`) or hits `unknown capability: module.load`
(`Engine.js`, `Physics.js`).

The existing coverage does not catch this: `tests/export_multi_test.rs`
(CRUSH-CI-HONESTY-1) asserts only that exported *names* reach CAST, not that
their definitions do.

## Reproduction

```sh
cargo run -p crush-lang-js --bin js_walker -- /workspace/projects/maya-engine/src/engine/scatter.js
# CAST main body is 4x Export { value: Var(..) } — hash/scatter/unit/clamp bodies absent

printf 'export function double(x) { return x * 2; }\nconsole.log(double(21));\n' > /tmp/exp.js
# walk -> CAST -> CASM -> CVM1 (see crush-lang-js/tests or crush-walk-run):
# compiles, then at runtime: load from uninitialised slot 0
```

## Success criteria

- [x] `export function f` puts `f` in `Program.functions` (callable from CAST/CASM/CVM1) in addition to the `Export` statement.
- [x] `export const/let/var x = <expr>` emits the `VarDecl`(s) for `x` in addition to the `Export` statement.
- [x] `export class C` emits the class `VarDecl` (via `lower_class_decl`) in addition to the `Export` statement.
- [x] `export default function/class` likewise retain their definitions.
- [x] New tests assert exported *definitions* (not just names) survive: an exported function called from `main` computes the right value end-to-end (CAST -> CASM), and exported var/class CAST shape is checked.
- [x] maya-engine spot check: `js_walker src/engine/scatter.js` CAST contains the `hash`/`scatter`/`clamp` bodies; `src/engine/Protocol.js` retains `validateScene`/`parseRecording`/`FrameStore`.
- [x] `cargo test -p crush-lang-js` green.

## Technical approach

- In `lower_module_decl`'s `ExportDecl` arm, lower the inner declaration first (reuse `lower_fn_decl` / `lower_var_declarator` / `lower_class_decl` and the module-level hoisting in `lower_module`), then append the existing `Export` statement(s).
- Mind ordering: `lower_module` hoists bare `function` declarations into `Program.functions` but keeps other top-level statements in `main`; exported functions must take the same path, exported vars/classes stay in `main` before their `Export`.
- Keep `ExportDefaultDecl` consistent (named default fn/class).

## Files to modify

- `crates/crush-lang-js/src/lower_swc.rs` — `lower_module_decl` / `lower_module` export handling.
- `crates/crush-lang-js/tests/export_multi_test.rs` (or a new `export_defs_test.rs`) — definition-preservation + end-to-end tests.

## Non-goals

- Module resolution / `module.load` (CRUSH-110), relative-ESM bundling, and any `three`/DOM/WebGL host capability — maya-engine still will not *run* in CVM1 after this fix; this ticket only stops the walker silently deleting the code it was asked to compile.
- Class semantics beyond today's `lower_class_decl` (constructor/`extends`/`this`) — CRUSH-251.
- Unsupported operators (`^`, `>>>`, `??`, `typeof`, `instanceof`) and destructuring declarations — separate gaps, found in the same audit; file separately if pursued.
- The Boa backend (`lower_boa.rs`) — same shape may apply; verify and file separately if it reproduces there.

## Resolution

Done on `agent/muse/CRUSH-266` (2026-10-20). `lower_module_decl` now returns
`(statements, hoisted_functions)`: exported functions hoist into
`Program.functions` via the shared `fn_decl_to_function` helper (the bare-`function`
path uses it too), exported vars emit `lower_var_declarator` before their
`Export`, exported classes emit `lower_class_body` before theirs, `export using`
keeps its initialiser, and named/anonymous `export default` fn/class retain
their definitions. New `crates/crush-lang-js/tests/export_defs_test.rs` (8 tests)
covers CAST shape + end-to-end runs; it needs CRUSH-266a (same branch) for the
`Export { Var(fn) }` load to execute.

Verified: `cargo test -p crush-lang-js -p crush-frontend` (301 passed),
`-p crush-lang-sdk -p crush-aot -p crush-aotc` (642 passed). maya-engine re-audit
with the fix: 27/37 walk fully and 13 now reach CASM with real bodies
(e.g. `Collision.js` 8 functions / 316 instrs, `labels.js` runs clean) — up from
0/37 faithful. The remaining failures are the honest next gaps, not this bug:
10 files now *fail loudly* at walk time on destructuring declarations inside
exported bodies (previously silently hollow), and others stop at `typeof` /
`instanceof` / `^` / `??` / CRUSH-251 captures in CASM. maya-engine still does
not *run* in CVM1 (no three/DOM/WebGL host, CRUSH-110 module loading) — unchanged,
and now visible instead of masked.

## Rebase note (2026-10-20)

Rebased onto `origin/main` (v0.7.1+; the branch was 131 commits behind).
Upstream had meanwhile made `lower_swc.rs` refuse unsupported constructs
loudly (CRUSH-zorro-23) and made `pat_to_name`/`lower_var_declarator`
fallible/kind-aware; the fix was ported onto that structure rather than
merged textually: upstream's `using` refusal and type-only-export handling
were kept, `export default class` reuses upstream's `lower_class`, and the
exported-var path passes the declaration kind through. CRUSH-266a turned out
to be already fixed upstream by CRUSH-250's `make_closure` function values
(see that ticket). `cargo test -p crush-lang-js --test export_defs_test
--test export_multi_test`: 10 passed on the rebased branch.

## Boa follow-up (2026-10-20)

The boa lowering (`crates/crush-lang-js/src/lower/stmt.rs`) had the same bug
in loud form: `export function` was refused outright (24 of 37 maya-engine
files failed there first). Fixed on this branch: exported function
declarations are fed into the script hoisting pass and `export()` emits only
the `Export` statement, mirroring the swc fix. Exported class/async/generator
stay loud (classes wait for COSM). Two tests in
`crates/crush-lang-js/tests/boa_backend_test.rs`. maya-engine boa sweep after
the fix: 1/37 walks end-to-end (Collision.js) — every other file now stops at
its *next* boa blocker (callbacks, `Object.*`, `new`/COSM, re-exports), which
is the expected queue, not this bug.
