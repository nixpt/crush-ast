# CRUSH-162 — In-process Lua `EXEC_LANG` (decision-gated)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-162 |
| **Priority** | P4 |
| **Status** | Backlog |
| **Phase** | M7 |
| **Assignee** | unassigned |
| **Dependencies** | decision C-2 |
| **Estimated effort** | M (~50 turns) |
| **Relay lane** | F — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

crush-ast has no Lua. nanovm ran Lua in-process (mlua, restricted stdlib `STRING|MATH|TABLE|COROUTINE`); exosphere's own Lua runtime had a no-op sandbox. CRUSH-55's W11 verdict was *retire*; this ticket exists only if the captain decides otherwise (C-2).

## Success criteria

- [ ] feature `lua` in crush-vm (off by default); `@lua { }` gated by `polyglot.lua`
- [ ] restricted stdlib (no `os`/`io`/`package`/`debug`), wall-clock + instruction limit, stdout captured
- [ ] variable marshaling consistent with the python/js sentinel protocol
- [ ] behaviour tests incl. an attempted `os.execute` being refused

## Technical approach

- Plug into `scheduler.rs`'s exec_lang and `portable_vm.rs` together (crush-diff must show 0 divergence).

## Files to modify

- `crates/crush-vm/Cargo.toml`
- `crates/crush-vm/src/{scheduler,portable_vm}.rs`

## Non-goals

- LuaJIT
- a Lua walker

## Source (reference only — re-implement, don't copy)

- exo `crates/core/vm/nanovm/src/polyglot/builtin_executors.rs`
