# CRUSH-174 — Browser playground on `crush-web` — check, then port only if missing

| Field | Value |
|-------|-------|
| **ID** | CRUSH-174 |
| **Priority** | P4 |
| **Status** | Superseded (2026-10-07) — crushlang.org/playground + CRUSH-118 |
| **Phase** | M8 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S (~20 turns) |
| **Relay lane** | E6 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

`nixpt/crush`'s `web/crush-web-ide` was a browser IDE shell. crush-web now has `Session`/`execute_with` (CRUSH-118) and foreman was wiring a playground on it. Avoid building a second one.

## Success criteria

- [x] confirm whether a playground already exists (crush-ast, crush-website, crush-language-guide) and record it here
- [ ] only if none: a static HTML/JS shell under `crates/crush-web/www/` using `Session`, verified in headless Chromium (+40 turns)

## Technical approach

- Check first; the IDE's Rust side targets an old API and would be rewritten.

## Files to modify

- `crates/crush-web/www/` (only if needed)

## Non-goals

- a full IDE

## Source (reference only — re-implement, don't copy)

- `nixpt/crush` `web/crush-web-ide`

## Resolution — Superseded (2026-10-07, captain s474, recorded by CRUSH-169)

A playground already exists: **https://crushlang.org/playground/** (HTTP 200 on 2026-10-07), built on
crush-web's `Session` / `execute_with` from **CRUSH-118** (crush-ast#93). Per this ticket's own rule
("port only if missing"), nothing is ported from `nixpt/crush`'s `web/crush-web-ide`.
