# CRUSH-44 — Snapshot/replay: .cvm-snapshot for PortableVM + FastVM

| Field | Value |
|-------|-------|
| **ID** | CRUSH-44 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M7 |

## Problem

No way to serialize VM state (mid-arena, mid-function-call) and replay it —
needed for crash forensics, long-running agent migration, and the
ai-native-roadmap ops story. No snapshot code exists (s412 triage).

## Approach

Spec first: `.cvm-snapshot` blob format (versioned; value heap/arena, stacks,
ip, quotas-remaining, pending host-call state or a "not-at-a-cap-boundary"
restriction — snapshot-at-safepoint is the cheap correct choice). Implement
serialize + deterministic replay for PortableVM and FastVM; JIT replay is
explicitly out of scope (restart jitted functions from safepoint via
interpreter). Round-trip test: run N steps, snapshot, restore, continue →
identical result to uninterrupted run (requires CRUSH-42).

## Definition of done

- [ ] Format doc + version field
- [ ] Snapshot-at-safepoint + restore for both VMs; round-trip equality test
      under `deterministic` feature
- [ ] Non-goals recorded (JIT replay, cross-version restore)

## Files in scope

- `crates/crush-vm` (new snapshot module), docs/design

## Gates

CRUSH-42 (deterministic mode).


## Dispatch

Imported from `workspace-meta/prompts/crush-backlog/CRUSH-44.txt` on 2026-08-24 so this tracked ticket contains the dispatch prompt metadata.

- Branch: `agent/horse/CRUSH-44`
- Repo: `crush-ast`
- Turns: `80`
- Runner: `claude`
- Scope: this ticket file is the canonical implementation spec; read it before changing code.
- Discipline: verify the repro against current `main` first, commit and push each meaningful unit, and update this ticket with the result.
- Verification: satisfy this ticket's Definition of done, include test evidence, and quote the real post-commit `HEAD` hash.
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs` unless this ticket explicitly scopes them; flag `crush_cast::Function`/`Program` shape changes before landing.
- Halt: stop and DM foreman if gates are unmet, scope is wrong, the repro no longer exists, sandbox blocks required work, or budget is nearly exhausted.
