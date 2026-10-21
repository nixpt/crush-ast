# CRUSH-276 — JS: what still blocks real ES modules (maya-engine as the corpus)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-276 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | JS/TS parity spike |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-266 (exports, both lowerings), CRUSH-zorro-18, CRUSH-zorro-20, CRUSH-zorro-44, COSM K3 (JS class slice) |
| **Estimated effort** | L (tracking ticket — the work is in the linked tickets) |
| **Filed by** | muse — 2026-10-20, maya-engine compile audit (after CRUSH-266) |

## Problem

With CRUSH-266 landed, exported declarations survive on both JS lowerings.
The next blockers for a real-world ES-module corpus are measurable. Corpus:
`maya-engine` (`worlds-workspace/maya-engine`), 37 ESM files in `src/`,
4,643 LOC, `three@0.180.0`. After CRUSH-266:

- **swc lowering** (current `.js` default): 3/37 walk; every refusal is loud.
- **boa lowering** (intended default, CRUSH-250 step 6): 1/37 walks
  (`engine/core/Collision.js`).

First blocker per file (swc): class `constructor` (19 files), chained
method calls `a.b.m()` (5), destructuring patterns (2 at top level, more
inside bodies), default parameter values (2), regex literals (1),
`new a.B()` (3), class `extends` (1). On boa the queue is the same plus
higher-order callbacks (`.reduce()`, zorro-20), `Object.*` (zorro-16) and
re-exports (`export … from …`).

## Success criteria

- [ ] Each blocker below either lands via its linked ticket or gets its own
      ticket with a maya-derived corpus case in the relevant parity corpus
      (`node_corpus` / `ts_corpus` / boa harness lists updated in the same PR).
- [ ] maya-engine walk counts (both lowerings) re-measured and recorded here
      whenever one of the linked items lands.
- [ ] No new silent drops: every unhandled construct stays a loud
      `unsupported(...)` refusal (CRUSH-238, CRUSH-zorro-23).

## Queue (in corpus-impact order)

1. **Classes** — `constructor`, `this`, `new`, `extends`: 19+ files.
   Waits for the COSM JS slice by design (`lower/mod.rs`: "Classes, `new`
   and `this` stay loud until COSM's JS slice"; `Construct::JsNew`,
   `Receiver::DynamicThis`, `Resolution::PrototypeChain` already exist in
   `crush-cosm`). Needs a K3 ticket: JS walker emits `CosmModule`, CAST
   `Program` carries it, `new` compiles to CONSTRUCT (zorro-43 ops exist).
2. **Method calls on non-name receivers** (`a.b.m()`, `f().m()`):
   zorro-44 (receiver resolution) on the frontend side; the swc lowering
   refuses at `lower_call_expr` because CAST `Call { function: String }`
   has no receiver operand.
3. **Destructuring declarations/parameters** (`const {a: b} = …`,
   `function f({x})`): refused in both lowerings; maya uses renaming and
   nested patterns (`{bounds_cells: [w, h]}`).
4. **Callbacks / closures as arguments** (`.reduce/.map/.filter`):
   zorro-20.
5. **Default parameters on the swc lowering**: zorro-18 did boa;
   zorro-26 covers swc/AOT divergence — or retire swc for `.js`
   (CRUSH-250 step 6) instead of fixing both.
6. **Regex literals**, **`Object.*`** (zorro-16), **re-exports**
   (`export { x } from …` — also CRUSH-110 module loading), **`new` on a
   member** (`new a.B()`).

## Non-goals

- Running three.js itself in a Crush VM (needs a three/DOM/WebGL host
  capability and CRUSH-110 module resolution — a product decision, not a
  walker gap). This ticket is about *compiling* the modules faithfully.
- Fixing the swc lowering where the boa lowering is the designated
  replacement, unless the step-6 flip slips.
