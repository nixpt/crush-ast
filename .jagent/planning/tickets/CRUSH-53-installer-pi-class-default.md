# CRUSH-53 — crush-installer: Pi-class (aarch64) default target + smoke test

| Field | Value |
|-------|-------|
| **ID** | CRUSH-53 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M8 |

## Problem

ROADMAP M8: Pi-class (`aarch64-unknown-linux-gnu`) should be a first-class
install target for embedded use. Verify crush-installer's current target list
at dispatch (crate exists, pre-dates the roadmap; no Pi work in git log per
s412 triage). ROADMAP's "gnueabihf" spelling is a 32-bit-ism — the ticket
standardizes on aarch64 (64-bit Pi 3+/4/5); note the correction.

## Approach

Add aarch64 to the installer target list (consuming CRUSH-51's shared platform
module); produce install artifacts from the CRUSH-50 lane; smoke test =
install + `crush-run` a hello fixture on real aarch64 (qemu-user acceptable,
labeled; real-Pi run manual, documented).

## Definition of done

- [ ] aarch64 installable; smoke green (qemu labeled if so)
- [ ] Target list documented; gnueabihf correction noted

## Files in scope

- `crates/crush-installer`

## Gates

CRUSH-50, CRUSH-51.


## Dispatch

Imported from `workspace-meta/prompts/crush-backlog/CRUSH-53.txt` on 2026-08-24 so this tracked ticket contains the dispatch prompt metadata.

- Branch: `agent/horse/CRUSH-53`
- Repo: `crush-ast`
- Turns: `40`
- Runner: `claude`
- Scope: this ticket file is the canonical implementation spec; read it before changing code.
- Discipline: verify the repro against current `main` first, commit and push each meaningful unit, and update this ticket with the result.
- Verification: satisfy this ticket's Definition of done, include test evidence, and quote the real post-commit `HEAD` hash.
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs` unless this ticket explicitly scopes them; flag `crush_cast::Function`/`Program` shape changes before landing.
- Halt: stop and DM foreman if gates are unmet, scope is wrong, the repro no longer exists, sandbox blocks required work, or budget is nearly exhausted.
