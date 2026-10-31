# CRUSH-342 — Keep tagged builds: pre-release per tag with prebuilt artifacts; cheaper PR release job

| Field | Value |
|-------|-------|
| **ID** | CRUSH-342 |
| **Priority** | P2 |
| **Status** | In review (PR open) |
| **Assignee** | nova |
| **Estimated effort** | M |

## Problem

None of crush-ast's 17 tags (newest v0.8.0) has a GitHub Release, so nothing built is kept.
The language guide's CI `cargo install`s crush-notebook-kernel from source every run (~1.7 min),
users compile crush-lang-sdk to get `crush`/`crush-run`, and the crush-web wasm the guide and
site ship is built by hand and committed. Meanwhile PR CI's `Build (release)` is the slowest
job (~9.4 of ~12 min wall time) and its output is discarded.

## Decisions (owner, 2026-10-31)

1. Every tag gets a Release marked **pre-release, not latest** (`--prerelease --latest=false`)
   carrying the build artifacts; a normal release from 1.0. Replaces the "pre-1.0: tag only" rule.
2. PR CI: `cargo check --release` of the same packages; full release build only on pushes to `main`
   and (via `artifacts.yml`) for tags.

## Success criteria

- [x] The CLI tarball includes `crush-gui` (apt-get pkg-config libfontconfig1-dev before the build).
- [x] `artifacts.yml` (workflow_dispatch, `tag` input): CLI tarball, crush-web pkg tarball, `SHA256SUMS`
      attached to the tag's pre-release; creates it if missing; idempotent (`--clobber`).
- [x] `release.yml` dispatches it for the tag it just pushed.
- [x] `[package.metadata.binstall]` on crush-lang-sdk and crush-pkg matches the asset name.
- [x] `bump-version.sh` policy text/behaviour updated; CONTRIBUTING + CHANGELOG.
- [x] PR CI release job is a `cargo check --release` on PRs.
- [ ] First real run (after merge): backfill v0.8.0 — `gh workflow run artifacts.yml --ref main -f tag=v0.8.0`.

## Follow-ups

- The squadron-side `bump-version` template needs the same policy change, or `--install` overwrites it.
- Other targets (aarch64, macOS) — Linux x86_64 only for now.
