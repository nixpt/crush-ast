# CRUSH-318 — wire `crush-net` into the toolchain as `tcp.*` caps behind their own grants

| Field | Value |
|-------|-------|
| **ID** | CRUSH-318 |
| **Priority** | P3 |
| **Status** | In Review (PR on `agent/nova/CRUSH-318`) |
| **Phase** | M9 |
| **Assignee** | nova |
| **Dependencies** | none (merges cleanly with, but is independent of, CRUSH-264) |
| **Estimated effort** | M |

## Problem

`crates/crush-net` (raw TCP/TLS sockets as HostCaps, ported from exosphere's
exo-mesh) compiled and passed its tests but nothing depended on it: no runner
registered its caps, so no Crush program could use them. It also depended on
`crush-lang-sdk`, which made it impossible for `crush-run` (in the SDK) to
depend on it. Owner decision (2026-10-31): wire it in **behind a separate
grant**; `--net` stays HTTP-client-only, and being a server (`listen`/`accept`)
needs its own explicit opt-in.

## Decision

- **Names:** `tcp.connect`, `tcp.tls_wrap`, `tcp.listen`, `tcp.accept`,
  `tcp.send`, `tcp.recv`, `tcp.close`, `tcp.ping`. Not `net.tcp_*`: `net.` is a
  family prefix in this codebase (`effects_of` matches `net.*`, `covers()` treats
  `net` as covering every `net.<x>`), so any `net.`-prefixed name would be
  swept up by the HTTP grant's effect label and by a `net` declaration in
  `@capabilities` / `capsule.toml`. Other grants are likewise one flag per namespace.
- **Grants:** `--tcp` (connect, tls_wrap) and `--tcp-listen` (listen, accept).
  `tcp.send/recv/close/ping` act only on an id a granted cap produced, so
  either flag registers them. `--tcp-listen` does not include `tcp.connect`.
- **Effects:** `tcp/connect`, `tcp/listen`, `tcp/io` (ping has none), never `net/http`.
- **Scoping:** none. No host:port allow-list (no precedent for per-argument
  scopes on a flag in `HostCapsBuilder`; `fs` uses a root dir, which has no
  socket analogue). Documented as all-or-nothing.
- **Dependency direction:** `crush-net` -> `crush-vm` only; `crush-lang-sdk`
  -> `crush-net` behind features `tcp` (plain) / `tcp-tls` (adds `tls_wrap`).
- **Default build:** `tcp` and `tcp-tls` are in the SDK's default features, so a
  stock `crush-run` has the flags; nothing is granted without the flag. The
  cost is rustls+ring+webpki-roots in the default build; `default-features =
  false` (wasm, embedders) drops it all.

## Success criteria

- [x] caps renamed `net.*` -> `tcp.*`; crush-net tests/docs updated
- [x] `--tcp` / `--tcp-listen` through `HostCapsBuilder` + effects table; refused before running, naming the flag
- [x] no dependency cycle (`cargo tree`/`cargo metadata`)
- [x] live loopback tests: echo client, listen+accept one connection, refusals, `--net` alone grants neither
- [x] docs: crush-net README, SDK README, root README, `crush-run --help`
- [x] dejavue decision recorded

## Files

- `crates/crush-net/` (`Cargo.toml`, `src/caps.rs`, `src/lib.rs`, tests, README)
- `crates/crush-lang-sdk/` (`Cargo.toml`, `src/host_caps.rs`, `src/effects.rs`, `src/bin/crush-run.rs`, `tests/tcp_caps.rs`)

## Non-goals / follow-ups noticed

- host:port scoping; DNS names in `tcp://` URIs (literal IPs only today)
- `tcp.recv` is UTF-8-only and blocking with no timeout
