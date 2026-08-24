# CRUSH-78 — Memory-model decision: cycles, GC, and which value model is truth

| Field | Value |
|-------|-------|
| **ID** | CRUSH-78 |
| **Priority** | P2 (design-first; no code until decided) |
| **Status** | Backlog |
| **Phase** | M10-prep |

## Problem

July research (finding #7): production PortableVm uses an `Rc<RefCell>` value
model that leaks reference cycles, while `memory.rs` carried a real mark-sweep
GC on a DIFFERENT value model that production never called — the
build-both-ends pattern again. The s412 triage then found **zero** hits for
`mark_sweep`/`garbage_collect` across `crates/` — the GC may since have been
deleted. FastVm has its own arena model. Nobody has written down which memory
model is truth or whether leaking cycles is accepted.

**First task at dispatch: establish current state** (what exists in crush-vm
today: Rc model? arena? any GC remnant?) before any position-taking.

## Approach

Decision ticket. Deliverable is a written, recorded decision — not code:
1. Survey: PortableVm value model, FastVm arena, JIT/AOT assumptions,
   allocation patterns of real workloads (CRUSH-71's bench baseline helps).
2. Options: (a) accept cycle leaks, document as a language contract;
   (b) wire/re-add a GC to the production model; (c) arena-unify around the
   FastVm model. Each with migration cost + what it does to M10's CRUSH-62.
3. Record via `dejavue decision` + a `docs/design/memory-model.md`.
4. File the implementation ticket(s) the decision implies.

## Definition of done

- [ ] Current-state survey committed (what models exist, what leaks)
- [ ] Decision recorded (dejavue + docs/design) with rationale + rejected options
- [ ] CRUSH-62 re-scoped or confirmed against the decision
- [ ] Implementation tickets filed

## Files in scope

- `docs/design/`, `.dejavue/` (writes); `crates/crush-vm/src/` (read-only survey)

## Gates

None. HARD-gates CRUSH-62.


## Dispatch

Imported from `workspace-meta/prompts/crush-backlog/CRUSH-78.txt` on 2026-08-24 so this tracked ticket contains the dispatch prompt metadata.

- Branch: `agent/horse/CRUSH-78`
- Repo: `crush-ast`
- Turns: `50`
- Runner: `claude`
- Scope: this ticket file is the canonical implementation spec; read it before changing code.
- Discipline: verify the repro against current `main` first, commit and push each meaningful unit, and update this ticket with the result.
- Verification: satisfy this ticket's Definition of done, include test evidence, and quote the real post-commit `HEAD` hash.
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs` unless this ticket explicitly scopes them; flag `crush_cast::Function`/`Program` shape changes before landing.
- Halt: stop and DM foreman if gates are unmet, scope is wrong, the repro no longer exists, sandbox blocks required work, or budget is nearly exhausted.
