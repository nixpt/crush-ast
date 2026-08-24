# CRUSH-51 — AOT/installer target_os + target_arch cfg audit (3 sites)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-51 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M8 |

## Problem

Three OS-cfg sites disagree on platform coverage (CRUSH-22's finding, expanded):
1. `crush-aot` compiler.rs: `.so`/`.dylib`/`.dll` branching
2. `crush-aotc` codegen.rs: unconditional `Command::new("cc")` (no cc on
   stock Windows)
3. `crush-installer` main.rs:466: its own separate Windows branch
Verify each site + current line numbers at dispatch. Silent disagreement means
"works on the OS the author used."

## Approach

One shared platform-info module (target_os/arch → lib extension, compiler
driver, install layout) consumed by all three; per-site unit tests; Windows
answer decided once (`cl.exe`? require MSYS? document unsupported?) and
recorded via dejavue decision, not implied differently in three places.

## Definition of done

- [ ] Shared module; all three sites consume it; disagreement impossible by
      construction
- [ ] Windows compiler-driver decision recorded
- [ ] Green on the CRUSH-49 matrix (or named xfails)

## Files in scope

- `crates/crush-aot`, `crates/crush-aotc`, `crates/crush-installer`, new shared module

## Gates

None; CRUSH-49 gives it CI teeth.


## Dispatch

Imported from `workspace-meta/prompts/crush-backlog/CRUSH-51.txt` on 2026-08-24 so this tracked ticket contains the dispatch prompt metadata.

- Branch: `agent/horse/CRUSH-51`
- Repo: `crush-ast`
- Turns: `60`
- Runner: `claude`
- Scope: this ticket file is the canonical implementation spec; read it before changing code.
- Discipline: verify the repro against current `main` first, commit and push each meaningful unit, and update this ticket with the result.
- Verification: satisfy this ticket's Definition of done, include test evidence, and quote the real post-commit `HEAD` hash.
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs` unless this ticket explicitly scopes them; flag `crush_cast::Function`/`Program` shape changes before landing.
- Halt: stop and DM foreman if gates are unmet, scope is wrong, the repro no longer exists, sandbox blocks required work, or budget is nearly exhausted.
