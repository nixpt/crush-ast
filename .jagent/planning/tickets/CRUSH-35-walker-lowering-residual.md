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

## Quick-win slice: VISION.md table refresh (buffy, 2026-10-16)

All 6 previously-cited "closed" sites re-verified in code on a current-main
worktree (paths moved since s412; new locations recorded):

1. List comprehensions — `crush-lang-python/src/lower_expr.rs:180` (ListComp/SetComp → `lower_list_or_set_comprehension`; filter-as-arg tests in `sdk.rs`).
2. Slice expressions — `crush-lang-python/src/lower_expr.rs:297,343` (Expr::Slice lower/upper/step).
3. Tuple unpacking — `crush-lang-python/src/lower_stmt.rs:672` (Expr::Tuple for-targets).
4. `range()` — `crush-frontend/src/compiler.rs:1906` (`range`/`make_range`).
5. `Math.floor` — `crush-lang-js/src/lower/builtins.rs:86` (`Math.floor` → `math.floor`, CRUSH-65).
6. Function calls with args — `crush-vm/src/fastvm/execution.rs:99` (`FastOp::Call`: caller-pushed args kept in place, `FastFrame` pushed; CRUSH-138's reversed-args bug is fixed).

`docs/benchmarks/VISION.md` "Walker Lowering Progress" refreshed with these
evidence refs; 6/7 rows now ✅, typed arrays (`Uint8Array`) remains the single
🔴 residual (confirmed: zero `Uint8Array` mentions in `crush-lang-js`).

**Ticket remains OPEN** — the DoD items (Uint8Array fixture, corpus fixtures)
are still unmet. Residual work is the real dispatch, not a quick win.
