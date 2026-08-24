# CRUSH-54 — Surfer's in-tree Crush runtime → crush-ast (waves 1+2)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-54 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M9 — CROSS-REPO (surfer-browser) |

## Problem

Surfer runs a tree-walk Crush interpreter (relocated from bliss-core in
EXO-RB) + an exosphere `crush-lang` path-dep — dual maintenance against
crush-ast, and surfer misses the bytecode VM/JIT entirely. Repo-root TASKS.md
"🔗 Cross-project" Tier-3 item, still open. Canonical seam:
`docs/design/exec-lang-pluggable-executor.md` (verify it exists; read before
design). Precondition ("crush-ast main stable+pushed") is met.

## Approach

Wave 1 (no behavior change): surfer consumes crush-ast crates as drop-in
(re-export shims where names differ), tree-walk still the executor; green =
surfer's existing script tests unchanged. Wave 2: swap execution onto
`crush_vm` (Value + HostCaps), delete the in-tree forks. Between waves, run
surfer's script corpus differentially old-vs-new (the CRUSH-73/77 tooling
pattern applied at the client). Surfer-side work happens in the surfer repo
under its own ticket; this ticket anchors the crush-ast side (API gaps found
→ filed here).

## Definition of done

- [ ] Wave 1 landed (surfer tests green, no fork edits thereafter)
- [ ] Wave 2 landed (no in-tree interpreter remains); differential corpus green
- [ ] crush-ast API gaps surfaced are filed as tickets

## Files in scope

- surfer-browser repo (branch there); crush-ast only for gap fixes

## Gates

M5–M7 surface stability (soft — wave 1 can start now). Coordinate with the
surfer-browser arc handoff doc (workspace memory).


## Dispatch

Imported from `workspace-meta/prompts/crush-backlog/CRUSH-54.txt` on 2026-08-24 so this tracked ticket contains the dispatch prompt metadata.

- Branch: `agent/horse/CRUSH-54`
- Repo: `surfer`
- Turns: `100`
- Runner: `claude`
- Scope: this ticket file is the canonical implementation spec; read it before changing code.
- Discipline: verify the repro against current `main` first, commit and push each meaningful unit, and update this ticket with the result.
- Verification: satisfy this ticket's Definition of done, include test evidence, and quote the real post-commit `HEAD` hash.
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs` unless this ticket explicitly scopes them; flag `crush_cast::Function`/`Program` shape changes before landing.
- Halt: stop and DM foreman if gates are unmet, scope is wrong, the repro no longer exists, sandbox blocks required work, or budget is nearly exhausted.
