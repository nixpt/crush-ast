# CRUSH-81 — SemanticAnalyzer: replace 4–14 full walks with Tarjan SCC inference order

| Field | Value |
|-------|-------|
| **ID** | CRUSH-81 |
| **Priority** | P1 |
| **Status** | Done — landed via CRUSH-71 (`11f7a1c` + foreman seed-fix `97bd7c4`) |
| **Phase** | Design/perf (s412) |

## Problem

Panini capture (2026-08-02): the SemanticAnalyzer runs 4–14 full AST walks
solely to work around HashMap function-iteration order during type inference
(`crush-frontend/src/semantics.rs:98-137`). Correct order is computable:
Tarjan SCC over the call graph, infer in reverse-topological order, fixpoint
only within a cycle's component.

⚠ **CHECK FIRST:** the live CRUSH-71 branch (`agent/panini-crush/CRUSH-71`)
was editing `semantics.rs` on 2026-08-02 — this may already be landed or
in-flight there. If landed: flip to verify+close citing the commit. If
in-flight: do not double-dispatch; wait for CRUSH-71 to merge.

## Approach

Build call graph during pass 1; Tarjan SCC (small, no dep needed); infer in
reverse-topo order; keep a bounded fixpoint within SCCs for mutual recursion.
Also removes a class of iteration-order nondeterminism (helps CRUSH-42/77).

## Definition of done

- [ ] Single-walk (plus per-SCC fixpoint) inference; walk-count assertion or
      instrumentation demonstrating the reduction
- [ ] Existing inference tests green (CRUSH-8/9's recursive/forward cases
      especially); `cargo test -p crush-frontend` green
- [ ] Bench delta on a many-function file recorded

## Files in scope

- `crates/crush-frontend/src/semantics.rs`

## Gates

CRUSH-71 merge status check (see warning above).

## Resolution (s412)

Implemented by the CRUSH-71 campaign itself, exactly as this ticket's warning
anticipated: call graph → iterative Tarjan SCC → reverse-topological
inference; non-recursive functions get ONE authoritative walk; only genuine
SCCs iterate. Measured (audit §4.1, median of 30): 3.4–3.9x on forward-chain
shapes, 1.3x on arith chains. Also fixed a latent correctness bug (10-iter
global cap could leave Null returns on >12-deep chains, HashMap-order
dependent). Post-merge, foreman fixed an SCC seeding bug (mutual recursion
typed as optional<T>; pre-seed members to Any — `97bd7c4`), caught by the
branch's own `mutual_recursion_return_types_resolve` test.


## Dispatch

Imported from `workspace-meta/prompts/crush-backlog/CRUSH-81.txt` on 2026-08-24 so this tracked ticket contains the dispatch prompt metadata.

- Branch: `agent/horse/CRUSH-81`
- Repo: `crush-ast`
- Turns: `80`
- Runner: `claude`
- Scope: this ticket file is the canonical implementation spec; read it before changing code.
- Discipline: verify the repro against current `main` first, commit and push each meaningful unit, and update this ticket with the result.
- Verification: satisfy this ticket's Definition of done, include test evidence, and quote the real post-commit `HEAD` hash.
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs` unless this ticket explicitly scopes them; flag `crush_cast::Function`/`Program` shape changes before landing.
- Halt: stop and DM foreman if gates are unmet, scope is wrong, the repro no longer exists, sandbox blocks required work, or budget is nearly exhausted.
