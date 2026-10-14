# CRUSH-264 — Stdlib defaults and exosphere compatibility: `net`/`db` off by default, no `http.*` alias, `time.now` unit

| Field | Value |
|-------|-------|
| **ID** | CRUSH-264 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M9 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S (mostly decisions + docs) |
| **Filed by** | claude — 2026-10-12 exosphere→crush-ast stdlib audit (v0.5.0 `c1186b8`) |

## Problem

The exosphere stdlib has been absorbed (CRUSH-122), but three compatibility decisions were never
made, so a default build silently lacks what exosphere programs expect:

1. **`net` and `db` are off by default.** `storage.*` was declined in favour of `db.query` /
   `db.execute` (CRUSH-154, C-6, `docs/design/storage-caps-declined.md`) and exosphere's `http.*`
   became `net.http_get/post/put/delete/request` (`crates/crush-lang-sdk/src/net.rs`) — but both
   sit behind features (`crush-lang-sdk/Cargo.toml`: `net = ["dep:ureq"]`, `db = ["dep:rusqlite"]`),
   `default` is `["native-plugins", "polyglot-python", "polyglot-javascript", "stdlib"]`, and no
   crate in the workspace turns them on. A default `crush-run` has neither the old caps nor their
   replacements.
2. **No `http.*` alias.** `examples/crush/httpclient.crush` still calls `http.get`.
3. **`time.now` unit.** exosphere/nanovm returned milliseconds; crush-ast's `time.now` returns
   seconds, with `time.now_ms` for milliseconds (`host_caps.rs`, CRUSH-122). That is settled in code
   but not stated in user-facing docs, so ported programs silently get values 1000× smaller.

## Decide

- Turn `net` and/or `db` on in the default `crush-run` / `crush` build (they are grant-gated
  anyway), or keep them opt-in and document the feature flags where the caps are listed.
- `http.*` → `net.*` alias (with a deprecation note), or a migration note plus updating the example.
- Document `time.now` = seconds / `time.now_ms` = milliseconds in the stdlib reference and
  MIGRATION-INVENTORY.

## Success criteria

- [ ] Decision recorded (dejavue decision + this ticket).
- [ ] Default-build behaviour matches the decision; `crush-run caps` lists what's available.
- [ ] `httpclient.crush` runs (or fails with a message naming the replacement).
- [ ] `time.now` unit documented.
