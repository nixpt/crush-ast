# CRUSH-157 — `ai_native.toolchain` strategy engine

| Field | Value |
|-------|-------|
| **ID** | CRUSH-157 |
| **Priority** | P2 |
| **Status** | Done (PR pending review) |
| **Phase** | M5 |
| **Assignee** | nimbus-b |
| **Dependencies** | CRUSH-156 |
| **Estimated effort** | M (~60 turns) |
| **Relay lane** | B2 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

crush-ast's AI tool-chain opcode is an echo stub. exosphere's engine (sequential / parallel / conditional / retry × fail-fast / continue / retry / fallback) is real but fleet-coupled (spawns joker-mcp, agent-mem). Port the engine, not the backends.

## Success criteria

- [x] `ai_native.toolchain` executes a tool list per strategy and error policy, dispatching **each step through `HostCaps::get(name)`** so grants apply per tool (an ungranted tool fails the step, never runs)
- [x] result shape `{results, aborted, abort_reason}`
- [x] tests: each strategy × each error policy with fake HostCaps, including an ungranted tool and a retry that succeeds on attempt 2
- [x] no shell-outs, no box paths

## Technical approach

- Give the toolchain cap access to the registry by construction (`Arc` snapshot of `HostCaps`), as CRUSH-55 App. C suggests.
- Parallel: `HostCap` is already `Send + Sync`.

## Files to modify

- `crates/crush-lang-sdk/src/ai_native.rs` (or `ai_native/toolchain.rs`)

## Non-goals

- joker-mcp / foreman-dispatch / agent-mem backends (host-supplied by ai-core)

## Source (reference only — re-implement, don't copy)

- exo `crates/platform/runtimes/ai/src/toolchain.rs` (412 L), `crates/core/vm/nanovm/src/vm/ai.rs`

## Evidence (nimbus-b, 2026-10-07)

- Engine: `crates/crush-lang-sdk/src/ai_native/toolchain.rs` (`ToolchainCap::new(Arc<HostCaps>)`),
  re-implemented from the inventory summary (no exosphere code copied). `ai_native::register`
  installs it with a snapshot of the registry at that point (`HostCaps` is now cheaply `Clone`,
  handlers are `Arc`-shared, so the snapshot shares cap state); the builder registers
  `ai_native` last, so every grant it made is visible and nothing else is.
- Grant gate per step: `tools.get(tool_name)` and, if set, `tools.get(required_capability)`;
  a refusal fails the step without calling anything and is never retried (a fallback can
  replace it). Arity is checked against the tool's spec; calls use `call_with_deadline`.
- Semantics chosen where the CAST didn't say: `retry` policy = `max_retries` more attempts
  (only when the error contains `retry_condition`, if given), then abort; `fallback` = try
  `fallback_tools` in order, then abort; parallel runs every step on scoped threads, then
  applies the policy in tool order; `retry` strategy's backoff doesn't sleep (deterministic).
  Args: `parameters.args` positional, else one map arg, empty → none; `"$binding"` substitution;
  conditions `name` / `!name` over bindings with CVM1 truthiness.
- Tests: 13 unit tests in `toolchain.rs` (each strategy × each policy with fake caps; ungranted
  tool under every strategy/policy; retry succeeding on attempt 2; required_capability; bindings;
  conditional positions; malformed payloads) + `tests/ai_toolchain.rs` (compiled CAST chain on
  scheduler and PortableVm: granted tool runs, `fs.write` refused and no file written, binding
  feeds the next step; builder installs the engine). `cargo test -p crush-vm -p crush-lang-sdk` green.
