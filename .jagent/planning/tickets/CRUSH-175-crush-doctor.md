# CRUSH-175 — `crush doctor` — polyglot runtime health check

| Field | Value |
|-------|-------|
| **ID** | CRUSH-175 |
| **Priority** | P4 |
| **Status** | Backlog |
| **Phase** | M7 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S (~25 turns) |
| **Relay lane** | E5 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

Polyglot needs host `python3`/`node`/`bash` (and `bwrap`/buckets for `sandboxed-polyglot`); nothing tells a user what is missing until a block fails. The old `crush-cli` had a `doctor` command (its joker/federation checks are out of scope).

## Success criteria

- [ ] `crush doctor` reports presence + version of python3, node, bash, bwrap, buckets, and which polyglot features this build has
- [ ] exit code non-zero when a granted-by-default runtime is missing; `--json` output
- [ ] test with a fake PATH

## Technical approach

- Subcommand in the `crush` umbrella (`crush-lang-sdk/src/bin/crush.rs`), reusing `resolve_lang_binary`.

## Files to modify

- `crates/crush-lang-sdk/src/bin/crush.rs` (+ a module)

## Non-goals

- installing anything

## Source (reference only — re-implement, don't copy)

- `nixpt/crush` `tools/crush-cli` (`doctor`)
