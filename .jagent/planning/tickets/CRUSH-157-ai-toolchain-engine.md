# CRUSH-157 — `ai_native.toolchain` strategy engine

| Field | Value |
|-------|-------|
| **ID** | CRUSH-157 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M5 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-156 |
| **Estimated effort** | M (~60 turns) |
| **Relay lane** | B2 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

crush-ast's AI tool-chain opcode is an echo stub. exosphere's engine (sequential / parallel / conditional / retry × fail-fast / continue / retry / fallback) is real but fleet-coupled (spawns joker-mcp, agent-mem). Port the engine, not the backends.

## Success criteria

- [ ] `ai_native.toolchain` executes a tool list per strategy and error policy, dispatching **each step through `HostCaps::get(name)`** so grants apply per tool (an ungranted tool fails the step, never runs)
- [ ] result shape `{results, aborted, abort_reason}`
- [ ] tests: each strategy × each error policy with fake HostCaps, including an ungranted tool and a retry that succeeds on attempt 2
- [ ] no shell-outs, no box paths

## Technical approach

- Give the toolchain cap access to the registry by construction (`Arc` snapshot of `HostCaps`), as CRUSH-55 App. C suggests.
- Parallel: `HostCap` is already `Send + Sync`.

## Files to modify

- `crates/crush-lang-sdk/src/ai_native.rs` (or `ai_native/toolchain.rs`)

## Non-goals

- joker-mcp / foreman-dispatch / agent-mem backends (host-supplied by ai-core)

## Source (reference only — re-implement, don't copy)

- exo `crates/platform/runtimes/ai/src/toolchain.rs` (412 L), `crates/core/vm/nanovm/src/vm/ai.rs`
