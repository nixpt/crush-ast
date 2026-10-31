# CRUSH-264 — Stdlib defaults and exosphere compatibility: `net`/`db` off by default, no `http.*` alias, `time.now` unit

| Field | Value |
|-------|-------|
| **ID** | CRUSH-264 |
| **Priority** | P2 |
| **Status** | Done (2026-10-31, nova) |
| **Phase** | M9 |
| **Assignee** | nova |
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

## Decisions (repo owner, 2026-10-31)

1. **`net` ON in the default build; `db` stays opt-in.** `crush-lang-sdk`'s `default` features
   gain `net` (ureq), so the default `crush-run` / `crush` binaries have `--net` →
   `net.http_get/post/put/delete/request`. Still grant-gated: without `--net` a program using
   `net.*` is refused before running. `db` (rusqlite) stays behind `--features db`; `crush-run caps`
   and `--db` say how to get it. *Choice of mechanism:* Cargo features are per-package, and the
   bins share a package with the library, so "binaries only" is not expressible without a new
   wrapper crate. The cost to library consumers that take default features is one small, pure-Rust
   HTTP client dependency (ureq) that is inert unless a host registers `net` caps; consumers that
   already opt out (`crush-web`, `crush-lang-js`) are unaffected, and embedders can use
   `default-features = false`.
2. **`http.*` → loud error, no alias.** `effects::RENAMED_CAPS` + `MissingGrant::renamed_to`;
   `describe_missing` prints `http.get was renamed to net.http_get (grant it with --net)`.
   Every preflight (crush-run, `crush run`, crush-web, notebook kernel) gets it for free.
   `examples/crush/httpclient.crush` now uses `net.http_get` (+ `str.format`, which also unblocked
   its compile error).
3. **`time.now` = seconds, `time.now_ms` = milliseconds** documented in `crush-run caps`, the
   `crush-lang-sdk` README and MIGRATION-INVENTORY.

## Success criteria

- [x] Decision recorded (dejavue decision + this ticket).
- [x] Default-build behaviour matches the decision; `crush-run caps` lists what's available.
- [x] `httpclient.crush` runs (or fails with a message naming the replacement).
- [x] `time.now` unit documented.
