# CRUSH-58 — Nakshatra artifact canonicalization (companion to CRUSH-23)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-58 |
| **Priority** | P3 (thin, mostly recording) |
| **Status** | Backlog |
| **Phase** | M9 |

## Problem

CRUSH-23 captured (not designed) the nakshatra half of the embedding story:
nakshatra has NO Crush engine of its own; its one real Crush artifact
(`tools/build.crush`) runs on exosphere's frozen in-tree path. Nothing records
that as the deliberate, canonical arrangement — so a future agent could
"helpfully" embed an engine in nakshatra or migrate the artifact wrongly.

## Approach

Recording ticket: (1) verify current state in the nakshatra repo (read-only —
where build.crush lives, what invokes it); (2) write the canonicalization
note (nakshatra = no sandboxed engine; build.crush runs on exosphere's path;
what would have to change to revisit) into CRUSH-23's ticket + a dejavue
decision in crush-ast AND a matching note in nakshatra's own repo memory;
(3) close CRUSH-23's nakshatra half.

## Definition of done

- [ ] Current state verified + cited
- [ ] Decision recorded in both repos' memory layers
- [ ] CRUSH-23 updated (nakshatra half closed)

## Files in scope

- Ticket files + dejavue entries; nakshatra repo read-only + its memory file

## Gates

None.


## Dispatch

Imported from `workspace-meta/prompts/crush-backlog/CRUSH-58.txt` on 2026-08-24 so this tracked ticket contains the dispatch prompt metadata.

- Branch: `agent/horse/CRUSH-58`
- Repo: `crush-ast`
- Turns: `30`
- Runner: `claude`
- Scope: this ticket file is the canonical implementation spec; read it before changing code.
- Discipline: verify the repro against current `main` first, commit and push each meaningful unit, and update this ticket with the result.
- Verification: satisfy this ticket's Definition of done, include test evidence, and quote the real post-commit `HEAD` hash.
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs` unless this ticket explicitly scopes them; flag `crush_cast::Function`/`Program` shape changes before landing.
- Halt: stop and DM foreman if gates are unmet, scope is wrong, the repro no longer exists, sandbox blocks required work, or budget is nearly exhausted.
