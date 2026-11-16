# CRUSH-500 — One FORMAT-VERSIONING page: minor/major rule for every on-disk format

| Field | Value |
|-------|-------|
| **ID** | CRUSH-500 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#500](https://github.com/nixpt/crush-ast/issues/500) |
| **Size** | S |
| **Dependencies** | CRUSH-457, CRUSH-458, CRUSH-479, CRUSH-478 |

## Problem

The versioning rule for each format is spread across code comments (bytecode.rs:14-22, pack.rs:15-33) and per-format notes in docs/FILE-FORMATS.md, with no single rule (RC gate F9, RELEASE-1.0-CRITERIA.md:99). Rulings 12-15 define 'additive = minor, breaking = major', but nothing writes down, per format (CAST, .castb, CASM JSON/.casmb, CVM1 container, COSM, crush.toml, lockfile), what counts as each kind of change and what a reader does with a newer minor.

## Evidence (at origin/main 7ecd2515)

- docs/FILE-FORMATS.md
- crates/crush-vm/src/bytecode.rs:14-22
- crates/crush-cast/src/pack.rs:15-33
- docs/planning/RELEASE-1.0-CRITERIA.md:93-99
- rulings 12-15

## Goal

docs/FORMAT-VERSIONING.md: a table of every on-disk format with its current version, what bumps minor vs major, reader behaviour on a newer minor/major, and the legacy-read policy, written after the sibling format tickets land so every row describes merged code. FILE-FORMATS.md links to it. RELEASE-1.0-CRITERIA.md is NOT edited (altair's docs PR owns it).

## Non-goals

- Code changes
- Editing the RC doc
- Deprecation policy (ruling 34)

## Files

- docs/FORMAT-VERSIONING.md (new)
- docs/FILE-FORMATS.md (link only)

## Done when

- docs/FORMAT-VERSIONING.md exists with one row per format in the docs/FILE-FORMATS.md table, plus COSM, crush.toml and capsule.lock
- every row cites the code constant/function it describes (path:line) and the ruling number
- FILE-FORMATS.md links to it

## Depends on

- CRUSH-457
- CRUSH-458
- CRUSH-479
- CRUSH-478

## Refs

- RC gate F9
- rulings 12-15

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
