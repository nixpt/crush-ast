# CRUSH-352 — `crush-pkg new NAME --cli` scaffolds a facet-based command-line program

| Field | Value |
|-------|-------|
| **ID** | CRUSH-352 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | M4 |
| **Assignee** | nova |
| **Dependencies** | CRUSH-351 (facet is a git dependency), CRUSH-348/349 (`run` sees it; `sys.exit` is the exit status) |
| **Estimated effort** | S |

## Problem

A new Crush program gets a CLI by hand-copying facet's README. `crush-pkg new` only knows
the hello-world template.

## Success criteria

- [x] `crush-pkg new NAME --cli` writes `capsule.toml` (category `cli`,
      `required = ["io.print","sys.args","sys.exit"]`, facet as a git dependency at
      `https://github.com/nixpt/facet` tag `v0.2.0`) and a `main.crush` with a working spec
      (a flag, a typed option with a default, a positional, `--help`).
- [x] `--facet-path DIR` uses a local checkout as a path dependency instead.
- [x] `cd NAME && crush-pkg run -- --help` prints facet's help and exits 0;
      `-- --bogus` exits 2 with facet's error.
- [x] CI-safe tests via `--facet-path` to a fixture.

## Resolution

Done on `agent/nova/CRUSH-351` (2026-10-31). `manifest::scaffold_cli_package` +
`FacetSource` (`FACET_GIT_URL`, `FACET_TAG` = `v0.2.0` are the pinned defaults; bump
when facet releases), `New { --cli, --facet-path }` in `main.rs`. Tests:
`tests/test_new_cli.rs` (3) against `tests/fixtures/mini-facet`. Live-checked with the real
facet (path and git tag).
