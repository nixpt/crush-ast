# Handoff

Updated: 2026-10-31 (vega). Earlier handoffs are in git history.

## In flight

- **#348** (no accidental 1.0): `bump-version.sh` bumps minor for a breaking change on 0.x; 1.0.0 needs
  `release.yml`'s `allow_major`. Awaiting review/merge. squadron's `bump-version` template needs the
  same change, or `bump-version --install` reverts it.
- **#281** (Crush 1.0 criteria, DRAFT for the captain): re-scored against `faeb86e8`.
- **0.9.0 release**: after #348 (#347 merged 2026-10-31, `73760920`) — dispatch `release.yml` (tag + pre-release + `artifacts.yml`),
  check the artifacts, then `publish.yml` with `tag=v0.9.0`; verify `cargo install crush-lang-sdk`.
- Optional backfill: `gh workflow run artifacts.yml --ref main -f tag=v0.8.0`.

## Next

- First `cargo publish` of the 15 never-published crates (walkers are still being worked on — hold
  until that settles), then Trusted Publishing for each on crates.io.
- CRUSH-343 (lexer turns unknown operator characters into identifiers).
- Runtime errors have no source position (no pc → source map).
- The 1.0 criteria's ❌ list (R1 first, then S1/S3/S5/S6, L1/L5/L7, Q5/Q7/Q8, D2).
