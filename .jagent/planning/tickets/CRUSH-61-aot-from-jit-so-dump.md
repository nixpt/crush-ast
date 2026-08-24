# CRUSH-61 — AOT-from-JIT: dump compiled native code as .so (cold-start-free deploys)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-61 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M10 |

## Problem

JIT compilation cost is paid per process start. ROADMAP M10 wants the JIT's
compiled output dumpable as a `.so` for cold-start-free deployment.
⚠ NAME COLLISION (s412 triage): git log's "M2 Phase 7" commits (9c4d2d5,
52c1e07) are JIT-into-differential-pipeline wiring — a DIFFERENT "Phase 7".
Do not grep those commits expecting AOT-dump work; none exists.

## Approach

Cranelift object-module path: compile FastOps via cranelift-object into a
relocatable object + link to .so; runtime stubs (host caps, value model)
resolved at load. Loader path in crush-lang-sdk (`crush-run --prejit x.so`).
Differential fixture: dumped .so result ≡ live-JIT ≡ interpreter. Note the
seam vs crush-aot's C backend honestly (two native paths; this one shares the
JIT's semantics by construction — that's its argument).

## Definition of done

- [ ] Dump + load + run works for corpus programs; differential-verified
- [ ] Cold-start bench: .so load vs JIT-compile quoted
- [ ] Relationship to crush-aot documented (when to use which)

## Files in scope

- `crates/crush-jit` (object emission), `crates/crush-lang-sdk` (loader)

## Gates

CRUSH-60.


## Dispatch

Imported from `workspace-meta/prompts/crush-backlog/CRUSH-61.txt` on 2026-08-24 so this tracked ticket contains the dispatch prompt metadata.

- Branch: `agent/horse/CRUSH-61`
- Repo: `crush-ast`
- Turns: `100`
- Runner: `claude`
- Scope: this ticket file is the canonical implementation spec; read it before changing code.
- Discipline: verify the repro against current `main` first, commit and push each meaningful unit, and update this ticket with the result.
- Verification: satisfy this ticket's Definition of done, include test evidence, and quote the real post-commit `HEAD` hash.
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs` unless this ticket explicitly scopes them; flag `crush_cast::Function`/`Program` shape changes before landing.
- Halt: stop and DM foreman if gates are unmet, scope is wrong, the repro no longer exists, sandbox blocks required work, or budget is nearly exhausted.
