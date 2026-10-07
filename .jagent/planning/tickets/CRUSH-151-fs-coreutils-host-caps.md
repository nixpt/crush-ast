# CRUSH-151 — fs coreutils host capabilities (`fs.ls/cat/pwd/mkdir/rm/cp/mv/touch/find`)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-151 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M9 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-113 (ordering only) |
| **Estimated effort** | M (~60 turns) |
| **Relay lane** | A2 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

crush-ast registers only `fs.read/write/exists/list` (`crates/crush-lang-sdk/src/host_caps.rs`). Three corpus programs are `expect-error` on the missing ones: `examples/crush/fs_test.crush` and `repl_test.crush` (`fs.pwd`), `phase2_3_test.crush` (`fs.touch`). exosphere's stdlib and the `nixpt/crush` ancestor both had the full coreutils family (corecap, fs grant).

## Success criteria

- [ ] `fs.ls` (alias of `fs.list`), `fs.cat` (alias of `fs.read`), `fs.pwd`, `fs.mkdir` (with parents flag), `fs.rm` (file; dir only with explicit recursive flag), `fs.cp`, `fs.mv`, `fs.touch`, `fs.find` registered behind the existing `--fs` grant
- [ ] every path goes through `resolve_path` (`host_caps.rs:254`) — a test per cap proves `../` and symlink escapes from `--fs-root` are refused, both for existing and not-yet-existing targets
- [ ] `fs.cd` per decision C-5 (recommended: VM-local cwd that later `fs.*` calls resolve against, clamped to `--fs-root`; never `chdir` the process) — or declined with a clear error
- [ ] `fs_test.crush`, `repl_test.crush`, `phase2_3_test.crush` lose their `expect-error` headers and pass under the conformance runner with `// caps: fs`
- [ ] `crush-run caps` lists the new caps; argc/returns in each `HostCapSpec`

## Technical approach

- Follow the existing `FsReadCap`/`FsWriteCap` pattern in `host_caps.rs` (one struct per cap, `spec()` + `call()`).
- Use the ancestor/exo semantics only as a behaviour reference (names, arg order) — re-implement over `std::fs`, not exo's HAL.
- `fs.find` returns an array of root-relative paths; cap result size by the existing quotas.

## Files to modify

- `crates/crush-lang-sdk/src/host_caps.rs` — new caps + registration under `fs`
- `crates/crush-lang-sdk/src/bin/crush-run.rs` — `caps` listing
- `examples/crush/{fs_test,repl_test,phase2_3_test}.crush` — headers
- `crates/crush-lang-sdk/tests/` — sandbox-escape tests

## Non-goals

- `chmod`/`chown`/`stat` (exo `core-utils` only; no Crush consumer)
- changing the process working directory

## Source (reference only — re-implement, don't copy)

- exo `crates/core/base/stdlib/src/fs.rs` (463 L); `nixpt/crush` `core/crush-stdlib/src/{fs,ics}.rs`
