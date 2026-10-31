# CRUSH-165 — CVM1 execution transcript (hash-chained), feature-gated (optional)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-165 |
| **Priority** | P5 |
| **Status** | Done |
| **Phase** | M7 |
| **Assignee** | satya |
| **Dependencies** | none |
| **Estimated effort** | S (~30 turns) |
| **Relay lane** | F — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

For attestation (NAK / exo-light), a run should be able to produce a tamper-evident transcript. nanovm's `audit.rs` hashed instructions but never recorded cap calls and was bypassed by FastVM; the ancestor's version hash-chained events.

## Success criteria

- [x] feature `transcript`: SHA-256 chain over executed opcodes **and** cap calls (name + arg hash + result hash)
- [x] identical transcript for identical runs; one-instruction difference changes it
- [x] zero cost when the feature is off

## Technical approach

- Design fresh in `scheduler.rs`/`portable_vm.rs`; do not port nanovm's half-wired version.

## Files to modify

- `crates/crush-vm/src/scheduler.rs`, `portable_vm.rs`, `Cargo.toml`

## Non-goals

- signing the transcript (host's job)

## Source (reference only — re-implement, don't copy)

- exo `crates/core/vm/nanovm/src/audit.rs`; `nixpt/crush` `core/nanovm/src/audit.rs`

## Resolution (2026-10-30, satya, branch `agent/satya/CRUSH-165`)

New off-by-default crush-vm feature `transcript` (optional `sha2` 0.10, already in
the graph via `casm`). Module `crates/crush-vm/src/transcript.rs` holds the
**encoding version 1** spec in its module doc: `h_0 = SHA256("crush-vm/cvm1-transcript/v1"
|| program_hash(code, consts))`, `h_n = SHA256(h_{n-1} || event)`; instruction event =
`0x01 || u64 ip || u32 len || opcode+operand bytes`; cap event = `0x02 || str(name) ||
SHA256(args) || outcome` (value hash / no value / error-text hash / accessor call);
canonical tagged value encoding with maps sorted by key and sets sorted by encoding.

Hook sites (all `#[cfg(feature = "transcript")]`):
- scheduler: instruction dispatch in `run_from` (before `execute_one`); `CAP_CALL`
  (index caps, `parallel_map`, `dispatch_cap`), `EXEC_LANG` (around `run_block`), AI and
  DOM opcode arms. Recorder is a thread-local scoped by `run_scheduled_with_transcript`;
  plain `run_scheduled*` and `parallel_map` workers pause it.
- PortableVm: instruction dispatch in `step` (un-recorded when a deferred `@lang` block
  yields, so the re-run is recorded once); the same cap arms; COSM print-hook nested VMs
  share the parent's chain.

API: `crush_vm::run_scheduled_with_transcript(..) -> (Result<VmResult, VmError>,
TranscriptDigest)`, `PortableVm::transcript_digest()`. Not covered: FastVM, crush-jit,
AOT backends; `parallel_map` worker internals (only its results); EXEC_LANG gate refusals
before the block runs. Cross-engine digests are not promised equal (error texts can differ);
they agree on every program the tests run on both engines, spawn/await included. Not wired into the CLI.
