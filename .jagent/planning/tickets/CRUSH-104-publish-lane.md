# CRUSH-104 — Make crush-lang-sdk publishable again (ship the polyglot gate; retire 0.2.x)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-104 |
| **Priority** | P1 (security: crates.io's SDK runs `@lang` blocks ungated) |
| **Status** | In review (panini, `agent/panini/CRUSH-104`) |
| **Phase** | Publish |

## Problem (rescoped 2026-10-07)

crates.io's newest `crush-lang-sdk` is **0.2.0** (2026-06-17). It requires
`crush-vm ^0.2.0`, and the only 0.2.x `crush-vm` (0.2.0) executes
`@<word> { code }` as `Command::new(word).arg("-c").arg(code)`
(`src/vm.rs:556` of the published crate): **no capability gate and no
allowlist**, so any `@word{}` block runs a PATH binary named `word` with the
host's authority. The `polyglot.<lang>` gate landed on main in `4d314c7`
(2026-07-14) and is in crates.io's `crush-vm` 0.3.0 and 0.3.6 (verified by
downloading both and reading `scheduler.rs`/`portable_vm.rs`), but no SDK
release ever picked it up, because the SDK's default features need
`crush-lang-python` and `crush-lang-js`, which were never published.

### The original gates are done

The pre-rescope ticket (version.workspace sweep, `walker` → `crush-walker`
rename, publish `walker-core`, wait on CRUSH-36) is obsolete for this goal:

- `crush-walker-core` is on crates.io (0.3.0, 2026-07-15) and is the only
  walker crate the SDK needs.
- Every crate in the SDK's publish set already uses `version.workspace = true`.
- The `crates/cli` (`walker`) rename and the remaining 0.1.0 walker crates
  aren't in the SDK's dependency closure, so they don't block this. That
  work stays with the walker consolidation (CRUSH-36 / M6), not here.

## Publish set (crates.io checked 2026-10-07)

The in-workspace closure of `crush-lang-sdk` with **default features**
(`native-plugins`, `polyglot-python`, `polyglot-javascript`), from
`cargo tree -p crush-lang-sdk -e normal,build`. Workspace version: 0.3.8.

| # | Crate | crates.io max | Usable as-is? | Publishable now? |
|---|-------|---------------|---------------|------------------|
| 1 | `crush-errors` | 0.3.7 | stale vs main | yes |
| 2 | `casm` | 0.3.7 | stale vs main | yes |
| 3 | `crush-diagnostics` | 0.3.7 | stale vs main | yes |
| 4 | `crush-ffi` | 0.3.7 | stale vs main | yes |
| 5 | `crush-vm` | 0.3.6 | gate **present** (0.3.0+); 0.2.0 ungated | yes |
| 6 | `crush-cson` | 0.3.0 | stale: main's is a thin `caison` wrapper | yes (`caison` 0.1.0 is on crates.io) |
| 7 | `crush-cast` | 0.3.0 | **no**: lacks `Statement::LangBlock.deps`, so main's `crush-lang-js` fails to compile against it (reproduced) | yes |
| 8 | `crush-index` | 0.3.0 | stale vs main | yes |
| 9 | `crush-frontend` | 0.3.0 | stale vs main | yes |
| 10 | `crush-walker-core` | 0.3.0 | stale vs main | yes |
| 11 | `crush-lang-js` | **none** (new name, unclaimed) | n/a | yes |
| 12 | `crush-lang-python` | **none** (new name, unclaimed) | n/a | yes |
| 13 | `crush-lang-sdk` | 0.2.0 (ungated via crush-vm 0.2.0) | n/a | yes |

"Publishable now?" means all of these hold: `description` + `license` +
`repository` present; every normal/build path dep also carries a `version`;
no `publish = false`; every non-workspace dependency is on crates.io. Optional
deps checked as well: `crush-buckets` 0.1.0 (crush-vm `sandboxed-polyglot`) and
`caison` 0.1.0 are both published. Dev-deps are path-only, which crates.io
allows (cargo strips them).

All 13 must be (re)published together. Several crates.io versions satisfy the
SDK's `^0.3.0` requirements by semver but not by API (row 7), and the SDK
itself needs 11 + 12.

### Topological publish order

`cargo package`'s own ordering (it sorts the selected packages by dependency):

```
crush-errors → casm → crush-diagnostics → crush-ffi → crush-vm → crush-cson
→ crush-cast → crush-index → crush-frontend → crush-walker-core
→ crush-lang-js → crush-lang-python → crush-lang-sdk
```

Edges, for review: crush-vm → {casm, crush-errors, crush-diagnostics,
crush-ffi}; crush-cson → {crush-vm, caison}; crush-cast → {crush-errors,
crush-cson}; crush-index → {crush-cast, crush-cson}; crush-frontend →
{crush-cast, casm, crush-errors, crush-index}; crush-walker-core → crush-cast;
crush-lang-{js,python} → {crush-cast, crush-walker-core}; crush-lang-sdk →
all of the above.

Rate limits: 11 updates (crates.io burst 30, refill 1/min) plus 2 new names
(burst 5, refill 1/10 min) fit in one sitting.

## Design call: publish the two walker crates (no default-feature change)

Option A: publish `crush-lang-python` + `crush-lang-js`.
Option B: drop `polyglot-python`/`polyglot-javascript` from the SDK's
defaults.

**Chose A.** Both crates package and verify cleanly, and their names are
unclaimed. B would silently change what `cargo add crush-lang-sdk` gets: with
those features off, `compile.rs::prepare_polyglot_blocks` skips free-variable
analysis, so Crush locals are not marshaled into `@python`/`@javascript`
blocks and the block's last binding is not marshaled back. `let base = 5;
@python { result = base * 2 }` would stop producing `result`. A keeps the
SDK's default behaviour and public API unchanged, so no halt was needed.

## Definition of done

- [x] Publish-set table + topological order, crates.io checked 2026-10-07
- [x] Every crate in the set packages and verifies (method + evidence below)
- [x] Gate test present and passing; `cargo test -p crush-lang-sdk` green
- [x] `Test (sdk)` CI job added (SDK tests were not in CI)
- [x] Internal path-dep version reqs raised `0.3.0` → `0.3.8` (see below)
- [x] Publish + yank commands written down, in order (below)
- [ ] Foreman merges; release workflow bumps + tags
- [ ] Foreman/captain publish the 13 crates in order
- [ ] `arniko-crush` released on `crush-lang-sdk ^0.3` (see yank preconditions)
- [ ] Captain yanks the 0.2.0 crates below

## Evidence

### Packaging: method

`cargo package` multi-package mode, cargo 1.98.1. With several `-p` flags it
packages the selected crates in dependency order and verifies each tarball
against a local overlay registry holding the other just-packaged tarballs.
This is the "newer cargo" route; no `[patch.crates-io]` scratch copy was
needed. Each crate's tarball is compiled from its extracted `.crate`, so this
is the same check `cargo publish --dry-run` does, without needing the upstream
crates to be on crates.io first.

```
export CARGO_TARGET_DIR=/build/target-panini-104 CARGO_BUILD_JOBS=2
cargo package --allow-dirty \
  -p crush-errors -p crush-diagnostics -p crush-ffi -p casm -p crush-vm \
  -p crush-cson -p crush-cast -p crush-index -p crush-frontend \
  -p crush-walker-core -p crush-lang-python -p crush-lang-js -p crush-lang-sdk
```

Result: exit 0. Every crate is `Packaged`, then `Verifying …` + `Finished`
for all 13, ending with
`Verifying crush-lang-sdk v0.3.8 … Finished dev profile … in 6m 18s`.
Only two cosmetic warnings: crush-frontend's bench path is outside the
package, and one `readme = "../../README.md"` shadowed by a local README.

Negative control: packaging only `-p crush-lang-js -p crush-lang-python`
makes cargo resolve `crush-cast` from crates.io (0.3.0), and verification
fails with `E0559: variant Statement::LangBlock has no field named deps`.
That failure is why all 13 go out together, in order.

`crates-publish-sync --dry-run` against this tree agrees on the first step:
`next: crush-diagnostics (update) v0.3.8`.

### Gate tests: `crates/crush-lang-sdk/tests/polyglot_capability_test.rs`

The existing tests cover `@bash`/`@python` refused without a grant and
`@python` running with `--polyglot`. CRUSH-104 adds:

- `unknown_lang_block_refused_without_grant`: `@zsh { touch probe }` (exactly
  the 0.2.0 escape: `zsh -c "touch probe"`) via `crush-run`, no flags.
- `unknown_lang_block_refused_even_with_polyglot`: same, with `--polyglot`.
- `embedded_runtime_refuses_lang_blocks_without_grant`: the embedder path
  (`Runtime` + `HostCapsBuilder::new().build()`) for bash/python/zsh.
- `embedded_runtime_refuses_unknown_lang_with_every_polyglot_grant`:
  `HostCapsBuilder::polyglot(&["python","javascript","bash"])`, `@zsh` still
  refused.

Each one asserts three things: the program fails, the error is the EXEC_LANG
gate/allowlist refusal (so a parse error can't pass by accident), and the
probe file was not created. A manual positive control (`@bash { touch probe }`
with `--polyglot`) does create the probe, so the probe check can fail.

`cargo test -p crush-lang-sdk` (default features), 2026-10-07: **315 passed,
0 failed, 1 ignored** across 29 test binaries;
`polyglot_capability_test`: 7 passed.

## Publish commands (foreman / captain, after merge + release tag)

Run from a clean checkout of the release tag the workflow creates, **not**
the shared checkout:

```bash
git -C /path/to/clean/crush-ast fetch --tags origin
git -C /path/to/clean/crush-ast checkout vX.Y.Z      # the tag release.yml made
cd /path/to/clean/crush-ast

# 1. Re-check on the tag (packages + verifies all 13 against each other)
cargo package -p crush-errors -p casm -p crush-diagnostics -p crush-ffi \
  -p crush-vm -p crush-cson -p crush-cast -p crush-index -p crush-frontend \
  -p crush-walker-core -p crush-lang-js -p crush-lang-python -p crush-lang-sdk

# 2. Publish, in this order (cargo publish waits for each to appear in the index)
cargo publish -p crush-errors
cargo publish -p casm
cargo publish -p crush-diagnostics
cargo publish -p crush-ffi
cargo publish -p crush-vm
cargo publish -p crush-cson
cargo publish -p crush-cast
cargo publish -p crush-index
cargo publish -p crush-frontend
cargo publish -p crush-walker-core
cargo publish -p crush-lang-js        # new crate name
cargo publish -p crush-lang-python    # new crate name
cargo publish -p crush-lang-sdk
```

Alternative: re-enable `crates-publish-sync.timer`, but its unit currently
points `--repo` at the shared checkout `/workspace/projects/crush-ast`, so it
publishes whatever that tree has checked out. Repoint it at a clean
origin/main tree first. The timer also walks the *whole* workspace in topo
order, so the SDK lands only after any earlier unpublished crates.

## Yank commands (captain, after the publish above)

Preconditions:

1. `crush-lang-sdk` ≥ 0.3.x is live on crates.io.
2. `arniko-crush` (crates.io 0.3.0, nixpt-owned) has a release on
   `crush-lang-sdk ^0.3`. Today it requires `^0.2.0`, so it currently pulls the
   ungated crush-vm 0.2.0, and yanking 0.2.0 breaks its fresh resolves.

Must yank (these carry, or force, the ungated spawn):

```bash
cargo yank --version 0.2.0 crush-lang-sdk
cargo yank --version 0.2.0 crush-vm
```

Optional, to retire the 0.2.x line (no ungated `@lang` spawn in these; their
only crates.io dependent is `crush-lang-sdk 0.2.0`):

```bash
cargo yank --version 0.2.0 crush-frontend
cargo yank --version 0.2.0 crush-cast
cargo yank --version 0.2.0 casm
cargo yank --version 0.2.0 crush-errors
```

(`crush-frontend` 0.2.0's `SubprocessWalker` does spawn PATH binaries, but
with fixed walker names such as `python_walker`, not a name taken from user
source. 0.3.0 has the same code, so it isn't a 0.2.x-specific issue.)

## Dependency requirement lower bounds

Every internal `path` dep that also carries `version = "0.3.0"` (the
`[workspace.dependencies]` table plus inline deps in 18 crate manifests;
`crush-buckets` excluded) now says `"0.3.8"`. A `0.3.0` requirement claims the
crates.io 0.3.0 releases are compatible, and they are not: row 7 above shows
`crush-lang-js` failing against `crush-cast` 0.3.0. A downstream lockfile that
already holds `crush-cast` 0.3.0 would keep it and fail to build. Cargo.lock
is unchanged, `cargo check --workspace` passes, and the 13-crate
`cargo package` re-verifies with the packaged manifests showing
`version = "0.3.8"`. `bump-version.sh` doesn't touch dependency requirements,
so these lower bounds stay put on later bumps. They only need raising again
when a dependent starts using a newer API.

## Files in scope

- Root `Cargo.toml` + 18 `crates/*/Cargo.toml` (dependency requirements only)

- `crates/crush-lang-sdk/tests/polyglot_capability_test.rs`
- `.github/workflows/ci.yml` (`Test (sdk)` job)
- `CHANGELOG.md`, `.jagent/planning/TASKS.md`, this ticket
