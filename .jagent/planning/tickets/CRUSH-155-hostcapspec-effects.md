# CRUSH-155 — Effect metadata on `HostCapSpec` (optional)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-155 |
| **Priority** | P4 |
| **Status** | Backlog |
| **Phase** | M9 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S (~20 turns) |
| **Relay lane** | A6 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

exosphere tagged every capability with `EffectRecord`s (`env/read`, `time/sleep`, `net/http`) via its ICS records. crush-ast's `HostCapSpec` (`crates/crush-vm/src/host.rs`) has name/argc/returns only, so tooling (CRUSH-170's capability inference, audit, osmosis grants) cannot ask "what does this cap touch?".

## Success criteria

- [ ] `HostCapSpec` gains an `effects` field (small enum or `&'static [&'static str]`), defaulted so the 60+ existing impls don't change
- [ ] the stdlib/host caps in `crush-lang-sdk` declare effects (pure caps: none)
- [ ] `crush-run caps --json` shows them

## Technical approach

- Additive, `#[non_exhaustive]`-safe change; no behaviour change.

## Files to modify

- `crates/crush-vm/src/host.rs`
- `crates/crush-lang-sdk/src/{host_caps,stdlib}.rs`
- `crates/crush-lang-sdk/src/bin/crush-run.rs`

## Non-goals

- porting ICBF / `ic_id` content hashes

## Source (reference only — re-implement, don't copy)

- exo `crates/core/base/stdlib/src/ics.rs`, `crates/core/base/common/src/icbf.rs`
