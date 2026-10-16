# CRUSH-111 — type checker's builtin table is out of sync with the compiler's

| Field | Value |
|-------|-------|
| **ID** | CRUSH-111 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none (see CRUSH-65 for the shared-registry recommendation) |
| **Estimated effort** | M |
| **Updated** | 2026-10-16 — walker vector measured in PR #263 review; registry state refreshed (see bottom) |

## Problem

`compiler.rs`'s `compile_call` handles ~25 builtin function names
(`str.contains`, `str.split`, `str.replace`, `str.join`, `make_range`,
`range`, `arr_get`, `arr_set`, `array.push`, `array.pop`, `math.pow`,
`math.sqrt`, …), but `semantics.rs` registers only `len` and `print` as
builtin functions — its `check()` inserts exactly those two names.

Because a non-dotted name like `arr_get(...)` or `make_range(...)` parses as
`Expression::Call` (not `CapabilityCall`), the **type checker** hits its
"Undefined function" bail before the compiler ever runs — so the compiler's
`compile_call` arms for those names are dead code from `.crush` source.

## Impact

The only array primitives callable from source are array literals, indexed
assignment (`xs[i] = v`), indexed read (`xs[i]`), and `str.split`. There is no
callable `make_range` / `append` / `push` / `arr_set` / `arr_get` *function*
form, so growing an array from source is effectively impossible — the root of
the integer-packing workarounds used across the `awesome-crush` games.

## Reproduction

```bash
printf 'fn main() { let r = make_range(1, 5); io.print(r); return 0; }\n' > /tmp/r.crush
crushc /tmp/r.crush -o /tmp/r.cvm1
# [type] type error: Undefined function: make_range

printf 'fn main() { let a = [1,2,3]; io.print(arr_get(a, 1)); return 0; }\n' > /tmp/g.crush
crushc /tmp/g.crush -o /tmp/g.cvm1
# [type] type error: Undefined function: arr_get
```

## Success criteria

- [ ] `make_range(a, b)`, `arr_get(a, i)`, `arr_set(a, i, v)` — and every other name `compile_call` already handles — type-check and run from `.crush` source.
- [ ] The builtin name table is defined in ONE place and consulted by both the type checker and the compiler (per CRUSH-65's "single shared builtin-name registry" recommendation).

## Technical approach

- Replace the two hardcoded `functions.insert("len"/"print")` calls in `semantics.rs::check()` with a shared builtin table (name → arg types → return type) that mirrors `compiler.rs`'s `compile_call` arms.
- Prefer the shared-registry direction from CRUSH-65 over a second hand-maintained list, so this class of drift can't recur.

## Files to modify

- `crates/crush-frontend/src/semantics.rs` — builtin registration.
- `crates/crush-frontend/src/compiler.rs` — source the same shared table.

## Non-goals

- Dotted `array.*`/`str.*`/`math.*` forms (those are CRUSH-112).
- Making `len()` accept strings (CRUSH-114).

---

## Walker vector + registry state (buffy, 2026-10-16, found in PR #263 review)

**Status update since filing** (measured on main `3ae0b4a`): the drift is still live, and its
loudest vector today is **walker output**, not `.crush` source.

- `semantics.rs:67-76` now registers `len`, `print` **and** `parallel_map` (the filing said
  "only `len` and `print`"; `parallel_map` arrived with #175).
- The single-registry direction this ticket asked for **partly landed**:
  `crates/crush-frontend/src/walker_intrinsics.rs` (`lookup` at `:64`) is consulted by the
  semantics pass (`semantics.rs:497`), the compiler (`compiler.rs:1750`) and the optimizer
  (`optimizer.rs:297,514`) — so semantic analysis and codegen *do* agree for the `__crush_*__`
  family. The registry stops there, though: the `compile_call` names this ticket lists
  (`make_range`, `range`, `arr_get`, `arr_set`, `array.push`/`.pop`, `str.*`, `math.*`,
  `ai_synthesize`, `ai_semantic_match`) are still special-cased **inline** in
  `compiler.rs:1778-2265` and remain invisible to the checker. Extending `walker_intrinsics`
  (or pointing both at one table) is now the concrete shape of this ticket.

### Measured divergence — same CAST, two frontend entry points (walker → VM)

| Program (Python, via `python_to_cast`) | `Compiler::new().compile` | `crush_frontend::compile_cast` |
|---|---|---|
| `for i in range(3): print(i)` | OK → `0 1 2` | **REFUSE `Undefined function: make_range`** |
| `xs = [i for i in range(3)]` | OK → `[0, 1, 2]` | **REFUSE `Undefined function: make_range`** |
| subscript / setindex / slice / `not` / `//` / `+=` / str-concat / `len` / user fn calls | OK | OK (agree) |
| JS `for`, `.length`, `+=`, `!` | OK | OK (agree) |

`make_range` is the only **observable** divergence found: it is the one walker-emitted intrinsic
name the compiler special-cases by name, and the checker's registry has never heard of it.

### Who uses which door (a fix must keep both green)

- **Checked** — `crush_frontend::compile_cast`: the SDK's public API
  (`crates/crush-lang-sdk/src/compile.rs`), `crush-lang-python/tests/pipeline_test.rs`,
  `crush-lang-js/tests/math_builtins_test.rs`, `crush-frontend/tests/{optimizer,type_check}_tests.rs`,
  `crush-aot/tests/differential_aot.rs:1195`.
- **Unchecked** — `Compiler::new().compile`: the walker CLI `crush-walk-run`
  (`crush-aot/src/bin/walk_run.rs:44`), `crush-aot/src/bin/aotc.rs:186`, every per-language SDK
  (`crush-lang-*/src/sdk.rs`, `crush-lang-sdk/src/bin/crushc.rs`), and the AOT parity harnesses
  (`crush-aot/tests/python_parity.rs`, `js_parity.rs`).

Consequence: no AOT/parity harness exercises the checked path for `range()`-based Python, so the
drift is invisible to CI — while the checked path is the one SDKs and embedders call.

### The workaround is already written down in a test

`crates/crush-lang-js/tests/length_test.rs:14-16`: *"The walker pipeline's compile step
(crush-walk-run / crush-aotc): it understands the walker's lowering helpers (`__crush_assign__`,
`make_range`) that `compile_cast`'s semantic check rejects."* That comment is this ticket.

### A third hand-maintained copy of the name list

The test-only SDK runners register NopCaps for the same family by hand
(`crush-lang-python/src/sdk.rs:31-38`, `crush-lang-js/src/sdk.rs:31-38`) and have already drifted
from each other and from the compiler's table (python's list lacks `__crush_setindex__`, which the
JS list has; both list `make_range`). One more reason the shared table should be the only list.

### Added success criteria

- [ ] `for i in range(3)` and list comprehensions compile through `compile_cast` — this clears the
      entry-point caveat currently recorded in VISION.md's walker table (CRUSH-35 / PR #263).
- [ ] A **parity test** proving the two doors agree: extend `crush-frontend/tests/walker_intrinsics.rs`
      (which already asserts both entry points behave identically for `__crush_assign__` and that
      `__crush_unknown__` stays an error on both) to walk a corpus of walker-emitted constructs
      through both, requiring identical accept/reject **and** identical output.
- [ ] `crush-lang-{python,js}/src/sdk.rs`'s hand-written cap lists derive from the shared table, or vanish.
- [ ] **Decide the canonical entry point** and write it down. Recommendation: keep `compile_cast`
      as *the* checked front door and make walker output pass it — the alternative (declaring the
      walker pipeline exempt from checking) trades a loud refusal for a silent unchecked fast lane.

### Adjacent finding (NOT this ticket — now minted)

`y = +5` / `~5` is refused by **both** doors — `Unsupported op: +` / `Unsupported op: ~` — so it is a
walker lowering gap, not drift; both entry points agree. Minted as **CRUSH-271** (2026-10-16,
same session, same measurements): Python *and* JS walkers emit the `UnaryOp` node while the C walker
emits the working `__crush_pos__` / `__crush_bit_not__` intrinsics.

---

### Related

- PR #263 review (vega, 2026-10-16) — where the walker vector was measured; the discussion in
  `VISION.md`'s walker table (`docs/benchmarks/VISION.md:135`) carries the caveat this ticket clears.
- CRUSH-35 — VISION.md table refresh; its `range()` row records the divergence.

## Dispatch (prepared 2026-10-16)

- Branch: `agent/panini/CRUSH-111`
- Repo: `crush-ast`
- Turns: `100`
- Runner: `claude`
- Identity: `panini` (compiler lane per this index's dispatch rules)
- Prompt: `workspace-meta/prompts/2026-10-16-CRUSH-111.txt` — linted clean
  (`dispatch-prompt lint` → 0 error / 0 warn / 0 info)
- Scope: this ticket file is the canonical spec. The deliverable is the **shared table** — extend
  `crates/crush-frontend/src/walker_intrinsics.rs` or replace it, but do not leave a second
  hand-maintained list behind.
- Environment: `/build` cleared to ~31 GB free at dispatch time (2026-10-16) — build with
  `CARGO_TARGET_DIR=/build/target-panini-crush-111 CARGO_BUILD_JOBS=2`; keep scratch off `/home` (7.7 GB free).
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs`; flag
  `crush_cast::Function`/`Program` shape changes before landing; do not edit the language walkers
  (`crates/crush-lang-*/src/**`) — this ticket fixes the receiving end.
- Discipline: incremental commits, quote the real post-commit `HEAD`.
- Verification: `range()` + a comprehension through **both** entry points with identical output, the
  new two-door parity test, and `unknown_helpers_remain_errors` still green.
- Halt: if the parity test shows the divergence is broader than `make_range` in a way that changes
  the fix's shape, or if making the doors agree requires a `crush_cast` shape change.
