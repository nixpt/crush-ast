# CRUSH-165 — CVM1 execution transcript (hash-chained), feature-gated (optional)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-165 |
| **Priority** | P5 |
| **Status** | Backlog |
| **Phase** | M7 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S (~30 turns) |
| **Relay lane** | F — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

For attestation (NAK / exo-light), a run should be able to produce a tamper-evident transcript. nanovm's `audit.rs` hashed instructions but never recorded cap calls and was bypassed by FastVM; the ancestor's version hash-chained events.

## Success criteria

- [ ] feature `transcript`: SHA-256 chain over executed opcodes **and** cap calls (name + arg hash + result hash)
- [ ] identical transcript for identical runs; one-instruction difference changes it
- [ ] zero cost when the feature is off

## Technical approach

- Design fresh in `scheduler.rs`/`portable_vm.rs`; do not port nanovm's half-wired version.

## Files to modify

- `crates/crush-vm/src/scheduler.rs`, `portable_vm.rs`, `Cargo.toml`

## Non-goals

- signing the transcript (host's job)

## Source (reference only — re-implement, don't copy)

- exo `crates/core/vm/nanovm/src/audit.rs`; `nixpt/crush` `core/nanovm/src/audit.rs`
