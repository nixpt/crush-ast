# CRUSH-153 — `env.all`/`env.home_dir` and `http.put/delete/request`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-153 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M9 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S (~30 turns) |
| **Relay lane** | A4 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

CRUSH-122's "homes recorded" list missed these exosphere corecaps. crush-ast has `env.get` (`--env`) and `net.http_get/http_post` (`net` feature, `crates/crush-lang-sdk/src/net.rs`).

## Success criteria

- [ ] `env.all` returns only the variables the `--env` grant exposes (never the full host environment unless the grant says so); `env.home_dir` behind the same grant
- [ ] `net.http_put`, `net.http_delete`, `net.http_request(method, url, body, headers)` behind the `net` feature, same timeout/deadline behaviour as `http_get`
- [ ] decide and document whether exo's `http.*` names get aliases (recommend: no — one name per capability)
- [ ] unit tests for each; a source-pipeline test for one env and one http cap (http against a local listener)

## Technical approach

- Extend `host_caps.rs` env registration and `net.rs`'s ureq client; keep one implementation per verb.

## Files to modify

- `crates/crush-lang-sdk/src/host_caps.rs`
- `crates/crush-lang-sdk/src/net.rs`

## Non-goals

- websockets / streaming bodies

## Source (reference only — re-implement, don't copy)

- exo `crates/core/base/stdlib/src/{env,http}.rs`
