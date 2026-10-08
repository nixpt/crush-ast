# CRUSH-162 — In-process Lua `EXEC_LANG` (decision-gated)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-162 |
| **Priority** | P4 |
| **Status** | Declined (2026-10-07) — captain decision C-2 |
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

## Resolution — Declined (2026-10-07, captain decision C-2, recorded by CRUSH-169)

Captain s474 (recorded on crush-ast#95): **no in-process Lua.** Do not reintroduce `mlua` or any
Lua runtime unless someone explicitly asks for it. Polyglot execution stays on the `EXEC_LANG`
subprocess path (+ buckets sandboxing). If a consumer ever asks, reopen this ticket — its success
criteria above are still the right bar (restricted stdlib, `polyglot.lua` gate, refusal test).
