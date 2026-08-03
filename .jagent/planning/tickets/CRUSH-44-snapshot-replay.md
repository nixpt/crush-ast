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
