# CRUSH-351 — crush-pkg git dependencies (`git` + `tag`/`rev`/`branch`, `capsule.lock`, `--offline`)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-351 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | M4 |
| **Assignee** | nova |
| **Dependencies** | CRUSH-348 (a dependency must be visible to `run`) |
| **Estimated effort** | M |

## Problem

`[[dependencies]]` only accepts `path`. A library such as `facet` that lives in its
own repository can only be consumed by a path to a local checkout, so a new Crush
program cannot get a CLI without first cloning facet by hand.

## Success criteria

- [x] `[[dependencies]]` accepts `git = URL` plus exactly one of `tag` / `rev` / `branch`;
      exactly one of `path` / `git` (both and neither are rejected).
- [x] Fetched with the `git` CLI into a cache (`$CRUSH_PKG_CACHE`, else
      `$XDG_CACHE_HOME/crush-pkg`, else `~/.cache/crush-pkg`), checked out at the exact
      commit, then resolved and concatenated like a path dependency.
- [x] Resolved commits recorded in `capsule.lock`; a later build reuses them (a moved tag
      does not change the build).
- [x] `--offline` / `CRUSH_PKG_OFFLINE=1` uses the lock and the cache only.
- [x] Clear errors: unknown tag/branch/revision, network failure (mentions `--offline`),
      dependency without a readable `entry`.
- [x] Tests use local `file://` repositories; no network.

## Resolution

Done on `agent/nova/CRUSH-351` (2026-10-31). New `crates/crush-pkg/src/gitdep.rs`
(cache layout `git/<host>/<repo>-<urlhash8>/<full-sha>/`, `LockEntry`, `resolve`),
`Dependency { git, tag, rev, branch }` + `Dependency::validate` in `manifest.rs`,
`PackageBuilder::offline` and the git branch of `resolve_deps` in `builder.rs`, global
`--offline` in `main.rs`. The unused-dependency lint (CRUSH-350) reads git dependencies
through the lockfile and cache, without fetching. Tests: `tests/test_git_deps.rs` (7).
Live-checked against `https://github.com/nixpt/facet` @ `v0.1.1` and a `file://` clone.
Not done: dependencies of dependencies are still not followed (as for path deps); no
`crush-pkg update` command (delete the lock entry to re-resolve).
