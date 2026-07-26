# CRUSH-65 — JS `Math.*` lowers to a name the compiler never matches (silent miscompile)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-65 (renumbered from CRUSH-39 — see Notes) |
| **Title** | JS `Math.*` case mismatch between `lower_swc.rs` and the builtin dispatch tables |
| **Hash** | (populated on commit) |
| **Status** | Done — fix landed, 11 tests green |
| **Phase** | Correctness — wrong-answer bug class (not a crash) |
| **Assignee** | panini |
| **Depends on** | none |
| **Blocks** | nothing filed; the shared-builtin-registry opportunity captured in TASKS.md is the natural follow-up |
| **Estimated effort** | ~4h, 1 commit |
| **Branch** | `agent/panini-crush/CRUSH-65` |

## Why this exists

JavaScript `Math.floor(x)` compiled, ran, and returned the **wrong number**. No error,
no warning, no panic. `docs/benchmarks/compute.js` printed **165** where it should print **465**.

The cause was a case mismatch across a double-maintained table boundary:

- **Producer** — `crates/crush-lang-js/src/lower_swc.rs:1272` matched the JS-side names
  (`"Math.floor"`, `"Math.sqrt"`, …) and emitted `Expression::Call { function: func_name }`,
  passing the **capitalized** name straight through, unmapped.
- **Consumers** — dispatch on **lowercase** names:
  - `crates/crush-frontend/src/compiler.rs:1416+` (`"math.pow"`, `"math.sqrt"`, `"math.abs"`,
    `"math.round"`, `"math.floor"`, `"math.ceil"`)
  - `crates/crush-aotc/src/codegen.rs:410-418` (`"math.sqrt"` → `cap_math_sqrt`, …)
  - `crates/crush-lang-sdk/src/stdlib.rs:339-350` (`math.*` host capabilities)

`"Math.floor"` never equals `"math.floor"`, so the call missed every builtin arm and fell
through to compiler.rs's **dotted method-call path** (`compiler.rs:1603-1612`): it emitted
`load Math` (an undeclared variable) followed by `cap_call floor`, which silently yielded a
zero-ish value instead of the floor. Hence 165 instead of 465.

The tell was the asymmetry inside the *same* match block: `"Array.isArray"` correctly maps to
`is_array` and `"JSON.parse"` maps to `json_parse`. Only the `Math.*` arm forgot to translate.
The intended design is therefore *map at lowering time*, which is what this ticket does —
rather than teaching every consumer table to accept both cases.

**Why it stayed silent: no test anywhere exercised JS `Math.*` end-to-end.** The fix is cheap;
the missing regression coverage is the part that actually mattered.

## Scope

| File | Change |
|------|--------|
| `crates/crush-lang-js/src/lower_swc.rs` | new `math_builtin()` helper (`Math.<op>` → `math.<op>`); split the single passthrough arm into two by *which consumer actually exists* — eight names to a capability call, `Math.random` left alone |
| `crates/crush-lang-js/Cargo.toml` | dev-dep `crush-lang-sdk` gains `features = ["stdlib"]` (registers the `math.*` host caps all eight mapped names now resolve through) |
| `crates/crush-lang-js/tests/math_builtins_test.rs` | new — 11 end-to-end tests asserting **numeric results**, incl. the `compute.js` → 465 case |

### The split, and why

| JS name | Lowers to | Consumer that justifies it |
|---------|-----------|----------------------------|
| `Math.abs` `Math.ceil` `Math.floor` `Math.pow` `Math.round` `Math.sqrt` `Math.min` `Math.max` | `Expression::CapabilityCall { name: "math.<op>" }` | the registered host caps in `stdlib.rs:32-42` — the only route that actually executes (see below) |
| `Math.random` | **not mapped** (falls through unchanged) | **no counterpart anywhere.** Not inventing one — see Findings. |

**Correction, established by running the tests (foreman, at finish).** The first attempt split
these two ways: `Call { "math.floor" }` for the six names with opcode arms in
`compiler.rs:1416+`, and `CapabilityCall` only for `min`/`max`. That was wrong, and the tests
caught it — the six `Call` cases failed with `Undefined function: math.floor` while `min`/`max`
passed.

The reason: **crush-frontend's semantic pass runs first and has no builtin registry at all.**
`semantics.rs:412-418` resolves an `Expression::Call`'s name against user-defined functions and
in-scope variables only, and `bail!`s on anything else. So *any* dotted name in a `Call` is
rejected as "Undefined function" before `compiler.rs`'s `math.floor`/`math.pow` arms are ever
consulted. **Those opcode arms are unreachable from this path** — reaching them requires the
name to already be a declared function, which `math.floor` never is.

So all eight lower as capability calls. This is a correctness-over-speed choice made under
duress: the opcode path would be faster, but it does not work, and making it work means giving
`semantics.rs` a builtin registry — which is finding #3 below, not this ticket.

Consequence for callers: JS using `Math.*` now requires the `stdlib` caps to be granted
(`HostCapsBuilder::new().stdlib(true)`), which is why the dev-dep gained that feature.

## Verification

`docs/benchmarks/compute.js`, hand-traced: `a=100, b=150, c=450, d=250,`
`e=floor(250/5)=50, f=127, g=254, h=154, i=155, j=465`.

Real commands + real output are recorded in the handoff and the commit message.

## Findings (audit of the same bug class in sibling table pairs)

All four are captured in `.jagent/planning/TASKS.md` via `dejavue plan`. **Fixed: only the
`Math.*` mismatch above.** Everything below is *found, left alone, deliberately*:

1. **`crush-aotc` vs `crush-frontend` dispatch on different IR forms entirely.**
   `crush-aotc/src/codegen.rs:410-418` dispatches math only on `cap_call` **names**
   (`"math.floor"`), but `compiler.rs` emits math as `math_*` **opcodes**. So AOT-via-`aotc`
   of any crush program using `math.floor` never reaches the `cap_math_*` arms — it hits the
   unknown-capability fallthrough at `codegen.rs:419` and pushes `CV_NULL`.
   `crush-aot/src/codegen_c.rs` is the mirror image: it handles the `math_*` opcode form
   (line 546) and silently stubs unknown `cap_call` to `mk_null()` (line 1045).
   **Same silent-wrong-answer class as this ticket, but structural (opcode-vs-capability IR
   mismatch), not a typo — out of scope per the halt criteria.**

2. **`Math.random` has no counterpart in the workspace.** No `math.random` arm in
   `compiler.rs`, no `MATH_RANDOM` opcode, no `MathRandomCap` in `stdlib.rs`, no `math.random`
   in `aotc`. `CryptoRandomCap` (`host_caps.rs:495`) yields random **bytes**, not a float in
   `[0,1)`, so it is not a substitute. Left unmapped rather than invented.

3. **Table asymmetry, not a bug:** `stdlib.rs` registers `math.sin`/`cos`/`tan`/`min`/`max`/`pi`;
   `compiler.rs` has opcode arms only for `pow`/`sqrt`/`abs`/`round`/`floor`/`ceil`. The others
   still work via the cap_call path, so this is **not** a correctness defect — but it is exactly
   the independently-maintained-table structure that produced this bug. A single shared
   builtin-name registry consumed by `lower_swc.rs` + `compiler.rs` + `aotc` + `aot` would close
   the whole class.

4. **`Math.sin`/`cos`/`tan` are absent from `lower_swc.rs` altogether**, so JS `Math.sin(x)`
   still silently miscompiles by the same mechanism. Not fixed here: those names were never in
   the producer list, so this is a *missing feature*, not the case-mismatch defect this ticket
   scopes. Adding names is scope growth; flagged instead.

5. **Float-vs-int rendering divergence.** `MATH_FLOOR` and friends push `Value::Float`
   (`scheduler.rs:664`), which `Display` renders as `"465.0"` (`vm.rs:274-279`). JS semantics give
   an integer, so JS `Math.floor(50.7)` prints `50.0` where node prints `50`. The tests therefore
   assert **numeric** equality, not string equality, so they won't fail for the wrong reason when
   this is addressed. Not fixed here — changing `MATH_*` return types touches VM semantics for
   every language frontend, well beyond this ticket.

## Notes

### ✅ ID collision — resolved

This work was originally dispatched as **CRUSH-39**, which was already reserved:
`ROADMAP.md:140` "Walker→AOT pipeline maturation for all 12 walkers", cross-referenced as a
downstream blocker in `CRUSH-36`, `CRUSH-37` (lines 12, 22, 72) and `CRUSH-38`
(lines 12, 22, 82, 118) — 9 references in total.

The collision was foreman's error: the ID was checked against `.jagent/planning/tickets/`
filenames but not against `ROADMAP.md`, where it was reserved without a ticket file existing
yet. panini-crush caught it mid-task and correctly escalated rather than renumbering another
agent's tickets unilaterally.

**Resolution: this ticket moved to CRUSH-65** (CRUSH-1..64 are all taken). Walker→AOT keeps
CRUSH-39 and all 9 existing cross-references remain valid — nothing of buffy's was rewritten.
The branch name `agent/panini-crush/CRUSH-39` is left as-dispatched rather than renamed, so it
still matches the bridge and event-log record of the dispatch.
