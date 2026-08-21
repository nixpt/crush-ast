# Changelog

All notable changes to this project are documented here.

Format loosely follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning is [SemVer](https://semver.org/), applied automatically by
`bump-version` (`scripts/bump-version.sh` + `.github/workflows/release.yml`) —
every push to `main` computes a bump from conventional-commit subjects since
the last `v*` tag and appends a mechanical entry below. Hand-written entries
are welcome for anything the mechanical summary won't capture well (a
behavioral note, a migration hint); the automation appends rather than
overwrites, so edit this file directly for that and let the next auto-bump
add its entry after yours.

## [Unreleased]

## [0.3.4] - 2026-08-21

- docs: rewrite CONTRIBUTING.md, add CHANGELOG.md, enrich CLAUDE.md/AGENTS.md (#51)



### Added

- `scripts/bump-version.sh` + `.github/workflows/release.yml` — automatic
  version bump + tag on every push to `main`.
- This file.

## [0.3.0] — 2026-0X-XX

Never tagged at the time (see `.jagent/planning/tickets/` / `STATE.md` for
context) — `Cargo.toml` and crates.io both moved to `0.3.0` without a
corresponding `v0.3.0` git tag. Noted here rather than silently starting the
changelog as if `0.3.0` didn't happen; `bump-version`'s automation picks up
cleanly from here regardless; the missing tag is a historical gap, not a
blocker.

## [0.2.0] and earlier

Predates this changelog. See `git log --oneline v0.2.0` and
`.jagent/planning/tickets/` for the historical record.
