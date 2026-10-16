# CRUSH-35 — Walker-lowering completion: residual = typed arrays + VISION.md table refresh

| Field | Value |
|-------|-------|
| **ID** | CRUSH-35 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M6 |

## Problem

VISION.md's Walker Lowering Progress table shows 7 red items — but the table
is STALE. s412 triage verified 6 of 7 already closed in code: list
comprehensions (crush-lang-python/src/sdk.rs:180, PYLOWER-1), slices
(lower_expr.rs:253 + CRUSH-7's 8f30a96), tuple unpacking (lower_stmt.rs:523),
range() (lower_expr.rs:379 + compiler.rs:1489), Math.floor (CRUSH-65
a612b7f), fn-call-with-args (fe9a60a frame parity). **Residual real work:
typed arrays (Uint8Array)** — zero hits anywhere. Re-verify the 6 closures by
reading the cited sites before asserting them (they're triage claims, not
gospel).

> **2026-10-16:** this file's s412 triage list was re-checked, and the
> instruction above was followed too weakly the first time — reading a cited
> line is not verification. Two claims are wrong (tuple unpacking is *not*
> lowered; comprehensions work only in three positions). See
> `## Quick-win slice … (corrected)` at the end for the runtime evidence.

## Approach

1. Typed arrays: decide representation (Value::Bytes exists since s298 —
   likely the backing store), lower Uint8Array construction/index/length from
   the JS walker; conformance fixtures.
2. VISION.md table refresh with per-item evidence commit/file:line — the
   table being wrong cost this milestone its shape once already.
3. Each of the 6 "closed" items gets a conformance-corpus fixture (CRUSH-73)
   so the table can't silently rot again.

## Definition of done

- [ ] Uint8Array fixture runs correctly through the real pipeline
- [ ] VISION.md table accurate with evidence per row
- [ ] 7 corpus fixtures committed (6 green + typed-array)

## Files in scope

- `crates/crush-lang-js` lowering, `crates/crush-vm` (only if Bytes indexing
  gaps), VISION.md

## Gates

None. Feeds CRUSH-103.


## Dispatch

Imported from `workspace-meta/prompts/crush-backlog/CRUSH-35.txt` on 2026-08-24 so this tracked ticket contains the dispatch prompt metadata.

- Branch: `agent/horse/CRUSH-35`
- Repo: `crush-ast`
- Turns: `60`
- Runner: `claude`
- Scope: this ticket file is the canonical implementation spec; read it before changing code.
- Discipline: verify the repro against current `main` first, commit and push each meaningful unit, and update this ticket with the result.
- Verification: satisfy this ticket's Definition of done, include test evidence, and quote the real post-commit `HEAD` hash.
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs` unless this ticket explicitly scopes them; flag `crush_cast::Function`/`Program` shape changes before landing.
- Halt: stop and DM foreman if gates are unmet, scope is wrong, the repro no longer exists, sandbox blocks required work, or budget is nearly exhausted.

## Quick-win slice: VISION.md table refresh (buffy, 2026-10-16 — corrected 2026-10-16)

### CORRECTION (after PR #263 review by vega)

The first pass of this note claimed **6/7 rows ✅** from *reading* the cited
sites. That was wrong: a line existing at a path is not evidence that the
feature lowers, and one cite (`lower_stmt.rs:672`) turned out to be
`exception_type_names` — the `except (A, B)` helper — not destructuring.
Vega re-ran every Python row through walker → compile → VM on the PR head and
compared against CPython; the double-check below reproduces that locally
(`cargo test -p crush-lang-python --test zz_probe_tmp`, probe deleted after use):

| Row | Runtime result | Verdict |
|-----|----------------|---------|
| `range()` | `for i in range(3)` → `0 1 2`; `range(5, 1, -2)` → `5 3` (via `Compiler::new().compile`) | ✅ |
| Tuple unpacking | `for a, b in …` → **walker refuse** `for loop target must be a name` (`lower_stmt.rs:346`); `a, b = 1, 2` → **walker refuse** `unsupported assignment target: Tuple(..)` (`lower_stmt.rs:226`) | 🔴 |
| List comprehensions | assign RHS ✅, filter ✅, call arg ✅, `return` ✅; `1 + len([i for i in range(4)])` → **walker refuse** `comprehension used in a nested expression position not yet supported` (`lower_expr.rs:190`) | 🟡 partial |
| Slice expressions | `[1,2,3,4][1:3][0]` → `2`; `[::2][1]` → `3` | ✅ |
| Function calls with args | `sub(10, 3)` (`a - b`) → `7` | ✅ |

JS side re-checked by running the suite, not grepping: `cargo test -p
crush-lang-js --test math_builtins_test` → **16 passed, 0 failed** (includes
`math_floor_returns_the_floor_not_zero`, `compute_benchmark_prints_465`).

Corrected counts: **4/7 ✅, 1 partial (comprehensions), 2 🔴** (tuple
unpacking, typed arrays).

### Entry-point divergence (new finding, may deserve its own ticket)

`range()` (and therefore every comprehension built on it) compiles through
`crush_frontend::compiler::Compiler::new().compile` but is **refused** by
`crush_frontend::compile_cast` with `Undefined function: make_range` — the
semantic analyzer's function check (`crush-frontend/src/semantics.rs:570`)
doesn't know the `make_range` intrinsic the compiler emits. `python_parity.rs`
uses the first path; `crush-lang-python/tests/pipeline_test.rs` uses the second.
Loud, not wrong, but the two front doors disagree — **this is CRUSH-111**
(builtin-table drift between the checker and the compiler; the shared
`walker_intrinsics` registry covers only the `__crush_*__` family). Evidence +
proposed parity test appended there on 2026-10-16; the VISION.md caveat clears
when it lands.

### Verified sites (post-correction)

1. List comprehensions — `crush-lang-python/src/lower_expr.rs:180` (ListComp/SetComp → `lower_list_or_set_comprehension`); nested-position refusal at `:190`. **Partial.**
2. Slice expressions — `crush-lang-python/src/lower_expr.rs:297,343` (Expr::Slice lower/upper/step). **Works.**
3. Tuple unpacking — refusals at `lower_stmt.rs:346` (for-target) and `:226` (assignment target). **Not lowered** — the s412 cite (`:672`) was unrelated.
4. `range()` — `crush-frontend/src/compiler.rs:1906` (`range`/`make_range`); works on the `Compiler` path only.
5. `Math.floor` — `crush-lang-js/src/lower/builtins.rs:86` (`Math.floor` → `math.floor`, CRUSH-65); suite-verified.
6. Function calls with args — `crush-vm/src/fastvm/execution.rs:99` (`FastOp::Call`; CRUSH-138's reversed-args bug is fixed); runtime-verified.

The table's previous "✅ Lowered" for tuple unpacking and comprehensions was
the kind of rot this ticket exists to stop — the per-row evidence column now
records *what was run*, not just what was read.

**Ticket remains OPEN** — and the residual is now bigger than typed arrays
alone: Uint8Array, **tuple-unpacking lowering**, comprehension nested
positions, and the `compile_cast`/`Compiler` divergence. The DoD items
(Uint8Array fixture, corpus fixtures) are still unmet; this was a table fix,
not the dispatch.
