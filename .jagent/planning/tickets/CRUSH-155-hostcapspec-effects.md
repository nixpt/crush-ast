# CRUSH-155 — Effect metadata on `HostCapSpec` (optional)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-155 |
| **Priority** | P4 |
| **Status** | Done (2026-10-07) |
| **Phase** | M9 |
| **Assignee** | nimbus (lane A6, derby phase 2) |
| **Dependencies** | none |
| **Estimated effort** | S (~20 turns) |
| **Relay lane** | A6 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

exosphere tagged every capability with `EffectRecord`s (`env/read`, `time/sleep`, `net/http`) via its ICS records. crush-ast's `HostCapSpec` (`crates/crush-vm/src/host.rs`) has name/argc/returns only, so tooling (CRUSH-170's capability inference, audit, osmosis grants) cannot ask "what does this cap touch?".

## Success criteria

- [x] `HostCapSpec` gains an `effects` field (small enum or `&'static [&'static str]`), defaulted so the 60+ existing impls don't change
- [x] the stdlib/host caps in `crush-lang-sdk` declare effects (pure caps: none)
- [x] `crush-run caps --json` shows them

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

## Resolution (2026-10-07, nimbus — lane A6)

- **Not a `HostCapSpec` field — a defaulted trait method.** `HostCapSpec` is a plain struct that
  every implementation (60+ here, more in exo-light / crush-notebook / crush-web) builds with a
  struct literal, so any new field breaks all of them; the ticket's own constraint ("defaulted so
  the existing impls don't change") rules a field out. `crush_vm::HostCap::effects() ->
  Option<&'static [&'static str]>` defaults to `None` (undeclared); `Some(&[])` = pure. Labels are
  `"<resource>/<action>"`. Purely informational — the VM never consults it.
- **crush-lang-sdk declares via one table, not 130 overrides:** `effects::effects_of(name)` covers
  every non-stdlib cap the builder can register; `HostCapsBuilder::build()` wraps handlers in a
  delegating `Declared` (spec / call / call_with_deadline unchanged). The stdlib is registered as a
  family and declared pure (incl. `system.*`). Polyglot gates (crush-vm) declare `process/spawn`.
  New additive `HostCaps::into_handlers()` makes the re-wrap possible.
- **Drift guard:** `every_builder_capability_declares_its_effects` builds a registry with every
  grant on (and, under `--all-features`, net/db/graphics) and fails on any undeclared cap —
  mutation-checked (dropping `env.get` from the table fails it).
- `crush-run caps --json`: one record per capability — `name`, `argc`, `returns`, `effects`,
  `grant` (`portable`, `always`, `stdlib (default; --no-stdlib)`, `--fs`, …). Portable built-ins:
  `io.print` → `stdout/write`, `io.read` → `stdin/read`.

Evidence: `cargo test -p crush-lang-sdk` (default and `--all-features` for `effects::`) green;
`crush_run_caps_json_lists_effects_and_grants`; live `crush-run caps --json` → 157 capabilities,
116 pure, 0 undeclared.
