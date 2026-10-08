# CRUSH-151 — fs coreutils host capabilities (`fs.ls/cat/pwd/mkdir/rm/cp/mv/touch/find`)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-151 |
| **Priority** | P2 |
| **Status** | Done (2026-10-07) |
| **Phase** | M9 |
| **Assignee** | nimbus (lane A2, derby phase 2) |
| **Dependencies** | CRUSH-113 (ordering only) |
| **Estimated effort** | M (~60 turns) |
| **Relay lane** | A2 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

crush-ast registers only `fs.read/write/exists/list` (`crates/crush-lang-sdk/src/host_caps.rs`). Three corpus programs are `expect-error` on the missing ones: `examples/crush/fs_test.crush` and `repl_test.crush` (`fs.pwd`), `phase2_3_test.crush` (`fs.touch`). exosphere's stdlib and the `nixpt/crush` ancestor both had the full coreutils family (corecap, fs grant).

## Success criteria

- [x] `fs.ls` (alias of `fs.list`), `fs.cat` (alias of `fs.read`), `fs.pwd`, `fs.mkdir` (with parents flag), `fs.rm` (file; dir only with explicit recursive flag), `fs.cp`, `fs.mv`, `fs.touch`, `fs.find` registered behind the existing `--fs` grant
- [x] every path goes through `resolve_path` (`host_caps.rs:254`) — a test per cap proves `../` and symlink escapes from `--fs-root` are refused, both for existing and not-yet-existing targets
- [x] `fs.cd` per decision C-5 (recommended: VM-local cwd that later `fs.*` calls resolve against, clamped to `--fs-root`; never `chdir` the process) — or declined with a clear error
- [x] `fs_test.crush`, `repl_test.crush`, `phase2_3_test.crush` lose their `expect-error` headers and pass under the conformance runner with `// caps: fs`
- [x] `crush-run caps` lists the new caps; argc/returns in each `HostCapSpec`

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

## Resolution (2026-10-07, nimbus — lane A2)

- `crates/crush-lang-sdk/src/fs_tools.rs`: `fs.ls [dir]`, `fs.cat`, `fs.pwd`, `fs.cd`,
  `fs.mkdir path [parents]`, `fs.rm path [recursive]`, `fs.cp src dst [recursive]`, `fs.mv`,
  `fs.touch`, `fs.find dir [glob]` — registered only by `HostCapsBuilder::fs(true)` / `--fs`.
- **`fs.cd` (decision C-5):** `host_caps::FsSandbox` = root + a working directory held by the
  registry (so per VM), shared by `fs.read/write/exists/list`, the coreutils and the `text.*`
  file tools. Paths resolve against it, then are confined to the root exactly as before (lexical
  `..` first, then symlinks through the deepest existing ancestor). The process cwd never moves
  (asserted in `cd_moves_a_vm_local_working_directory`). `fs.pwd` is root-relative (`.` at the
  root) — no host path leaks; absolute paths are still refused everywhere.
- Safety rules beyond the ticket: `fs.rm`/`fs.mv` resolve the last component without following a
  symlink (they act on the link); `fs.rm`/`fs.mv` refuse the root, the working directory and its
  ancestors; directories need the explicit recursive flag; recursive `fs.cp` refuses symlinks and
  copying a directory into itself; `fs.find` does not descend through symlinks and stops at
  `FIND_MAX_RESULTS` (10 000) — a HostCap cannot see the VM's quotas, so a fixed cap stands in.
- `crush-vm` `PRIVILEGED_PREFIXES`: `fs.mkdir/rm/cp/mv/touch` join `fs.write`, so PortableVm's
  privileged tier cannot be bypassed by the new write paths.
- `fs.find` returns paths in the form the caller can pass back (relative to the working
  directory, prefixed with the searched dir, like Unix `find`), not root-relative as the ticket
  sketched — root-relative paths break as soon as the working directory is not the root.
- Examples: `fs_test`, `repl_test`, `phase2_3_test` now `// caps: fs` + `// expect:` and pass the
  conformance runner (3/3, leaving the repo clean). Their `text.echo(..., true)` calls became
  `io.print` (inventory §2: `text.echo` is a dead duplicate of `io.print`); the two "find" steps
  that only printed a heading now call `fs.find`.

Evidence: `cargo test -p crush-lang-sdk` green — `fs_tools::tests` (10, incl.
`every_path_argument_is_confined_to_the_root` for all 10 caps × `..`/absolute/symlink, existing and
new targets; mutation check: dropping the containment test fails it), `crush_run_fs_coreutils_stay_in_the_sandbox`,
`crush_run_caps_lists_fs_coreutils`; `cargo test -p crush-vm --lib caps::`.
