# CRUSH-52 — Android API host cap shard (crush-lang-android)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-52 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M8 |

## Problem

ROADMAP M8: Android as a real target — `crush-lang-android` host caps + a
sample Crush app on an Android emulator with an end-to-end test. Today: no
such crate; and the JVM-bridge groundwork lives in the CRUSH-21 java-kotlin
family, whose JVM/Android-guest-bridge sub-shard is STILL UNFILED (s412
triage) — that sub-shard is a real gate.

## Approach

1. File + land the CRUSH-21 JVM-bridge sub-shard first (gate).
2. `crush-lang-android`: host caps for the sanctioned Android surface
   (log, storage-scoped fs, sensors TBD) via JNI on the JVM bridge —
   capability-mediated like every host surface.
3. Sample app + emulator e2e in a nightly/manual CI lane (emulator in PR CI
   is flake-bait; explicitly nightly).

## Definition of done

- [ ] Sub-shard gate filed + met
- [ ] Crate + sample app; emulator e2e green in nightly lane
- [ ] aarch64 build path proven (CRUSH-50 lane)

## Files in scope

- New `crates/crush-lang-android`; CRUSH-21 family for the bridge

## Gates

CRUSH-21's JVM-bridge sub-shard (unfiled — file it as CRUSH-105); CRUSH-50 aarch64 lane.


## Dispatch

Imported from `workspace-meta/prompts/crush-backlog/CRUSH-52.txt` on 2026-08-24 so this tracked ticket contains the dispatch prompt metadata.

- Branch: `agent/horse/CRUSH-52`
- Repo: `crush-ast`
- Turns: `80`
- Runner: `claude`
- Scope: this ticket file is the canonical implementation spec; read it before changing code.
- Discipline: verify the repro against current `main` first, commit and push each meaningful unit, and update this ticket with the result.
- Verification: satisfy this ticket's Definition of done, include test evidence, and quote the real post-commit `HEAD` hash.
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs` unless this ticket explicitly scopes them; flag `crush_cast::Function`/`Program` shape changes before landing.
- Halt: stop and DM foreman if gates are unmet, scope is wrong, the repro no longer exists, sandbox blocks required work, or budget is nearly exhausted.
