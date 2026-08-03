# CRUSH-40 — Cooperative wall-clock timeout for ALL remaining blocking caps

| Field | Value |
|-------|-------|
| **ID** | CRUSH-40 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M7 |

## Problem

A stuck host call hangs the VM forever. CRUSH-19 fixed CAP_CALL
(`HostCap::call_with_deadline`, crush-vm/src/host.rs:65) and
CRUSHAST-CAPTIMEOUT-1 (401abe1) covered EXEC_LANG. **Residual scope** (s412
triage): IO_READ, IO_WRITE, NET_CONNECT, PROCESS_WAIT, HOST_REQUEST — plus an
audit pass enumerating every actually-blocking handler in `scheduler.rs` /
`portable_vm.rs` so the list is pinned by code, not by this ticket's guess.

## Approach

Extend the CRUSH-19 cooperative-deadline shape (self-enforcing
`Quotas::max_wall_time_ms`, `HostCapError::Timeout` → `VmError::CapTimeout`) to
each remaining site; per-site regression test with a genuinely-blocking impl
asserting prompt timeout (not a hang) — CRUSH-19's test is the template.

## Definition of done

- [ ] Audit table committed: every blocking opcode/cap → covered-by
- [ ] Each residual site enforces the deadline; per-site blocking-impl test
- [ ] `cargo test -p crush-vm` green (both feature sets)

## Files in scope

- `crates/crush-vm/src/scheduler.rs`, `portable_vm.rs`, `host.rs`

## Gates

None.
