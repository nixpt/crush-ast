# CRUSH-153 — `env.all`/`env.home_dir` and `http.put/delete/request`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-153 |
| **Priority** | P3 |
| **Status** | Done (2026-10-07) |
| **Phase** | M9 |
| **Assignee** | nimbus (lane A4, derby phase 2) |
| **Dependencies** | none |
| **Estimated effort** | S (~30 turns) |
| **Relay lane** | A4 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

CRUSH-122's "homes recorded" list missed these exosphere corecaps. crush-ast has `env.get` (`--env`) and `net.http_get/http_post` (`net` feature, `crates/crush-lang-sdk/src/net.rs`).

## Success criteria

- [x] `env.all` returns only the variables the `--env` grant exposes (never the full host environment unless the grant says so); `env.home_dir` behind the same grant
- [x] `net.http_put`, `net.http_delete`, `net.http_request(method, url, body, headers)` behind the `net` feature, same timeout/deadline behaviour as `http_get`
- [x] decide and document whether exo's `http.*` names get aliases (recommend: no — one name per capability)
- [x] unit tests for each; a source-pipeline test for one env and one http cap (http against a local listener)

## Technical approach

- Extend `host_caps.rs` env registration and `net.rs`'s ureq client; keep one implementation per verb.

## Files to modify

- `crates/crush-lang-sdk/src/host_caps.rs`
- `crates/crush-lang-sdk/src/net.rs`

## Non-goals

- websockets / streaming bodies

## Source (reference only — re-implement, don't copy)

- exo `crates/core/base/stdlib/src/{env,http}.rs`

## Resolution (2026-10-07, nimbus — lane A4)

- **env** (`host_caps.rs`, `--env`): `env.all()` → map of the host environment with the
  builder's `with_env_var` values on top (non-Unicode entries skipped); `env.home_dir()` →
  `HOME` (`USERPROFILE` on Windows) through the same overrides, or null. The `--env` grant
  already exposed the whole host environment to `env.get`, so `env.all` exposes exactly that —
  nothing more.
- **net** (`net.rs`, `net` feature + `--net`): `net.http_put(url, body)`,
  `net.http_delete(url)`, `net.http_request(method, url, body, headers)`. All five verbs run
  through one `request()` (one implementation per verb, as asked). `http_request` returns
  `{status, body}` and does not fail on a 4xx/5xx; the four fixed verbs keep returning the body
  and failing on an error status. `body` may be null; `headers` is a map or null; the method
  must be letters only.
- **"Same timeout/deadline behaviour as http_get":** http_get had none — it ignored
  `max_wall_time_ms`. Now every verb overrides `call_with_deadline`, sets the ureq agent
  timeout to the remaining budget, and maps an I/O timeout to `HostCapError::Timeout`
  (→ `VmError::CapTimeout`). This also fixes http_get/http_post.
- **Aliases: no.** exosphere's `http.*` names are not registered — one name per capability
  (documented in `net.rs`' module header). Public `NetHttpGetCap::new` / `NetHttpPostCap::new`
  unchanged.

Evidence (`cargo test -p crush-lang-sdk --features net`): `put_and_delete_send_their_method_and_body`,
`request_sends_method_headers_body_and_reports_status`, `get_still_fails_on_an_error_status`,
`every_verb_times_out_at_the_deadline` (local server waits 3 s, deadline 150 ms, all 5 verbs
return `Timeout` in < 2 s), `http_put_through_the_source_pipeline` (+ refusal without the grant),
`env_all_and_home_dir_follow_the_env_grant` (+ refusal), `env_home_dir_through_the_source_pipeline`.
Note: CI's `Test (sdk)` job runs default features only, so the `net` tests run locally, not in CI.
