# CRUSH-105 — JVM/Android guest bridge (the unfiled CRUSH-21 sub-shard)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-105 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M6/M8 seam |

## Problem

CRUSH-21 (java-kotlin family) split its walker half into CRUSH-37/38, but its
JVM/Android-guest-bridge sub-shard was never filed (s412 triage) — and
CRUSH-52 (Android host caps) gates on it. Scope: how Crush programs CALL INTO
and GET CALLED FROM JVM code (JNI surface, value marshaling for the
Value↔JVM-type seam, lifecycle) — distinct from parsing Java/Kotlin source
(37/38's lane).

## Approach

Read CRUSH-21's ticket for the captured family design first. Spec the bridge:
JNI-based host-cap provider pattern (capability-mediated like everything
else), typed marshaling reusing CRUSH-68's polyglot marshaling shapes, and an
embedding story (JVM app hosts crush-vm via the existing crush-vm-capi or a
thin JNI layer over it — decide which, record via dejavue). PoC: JVM test
calling a crush function and vice versa.

## Definition of done

- [ ] Bridge design recorded (capi-vs-JNI decision + marshaling table)
- [ ] Round-trip PoC test green on desktop JVM
- [ ] CRUSH-52's gate satisfied (Android build of the same bridge proven or
      residual named)

## Files in scope

- `crates/crush-vm-capi` and/or new JNI layer; CRUSH-21 family docs

## Gates

CRUSH-37 usefully first (shared family context), not strictly required.
