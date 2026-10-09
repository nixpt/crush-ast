# CRUSH-204 — Walker CLIs: `crush-walk-run` has no stdlib caps; `walker` needs PATH; `--help` treated as a filename; `export-py` writes the tree

| Field | Value |
|-------|-------|
| **ID** | CRUSH-204 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M6 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

- `crush-walk-run` registers no stdlib host caps: JS `Math.floor(3.5)` →
  `unknown capability: math.floor`, so CRUSH-65/69's walker fixes are invisible through it.
- `walker` (`crates/cli/src/main.rs`) launches per-language walkers by bare name from
  `PATH` → `Failed to execute python_walker` unless `target/debug` is on PATH.
  `-f casm` is ignored (still CAST JSON); `-f bogus` accepted; `.java`, `.dart`, `.np` unmapped.
- `python_walker`, `js_walker`, `rust_walker`, `bash_walker`, `zsh_walker --help`
  treat `--help` as a filename. `python_walker` with no args panics.
- `export-py` has no CLI: `export-py --help` runs anyway and rewrites
  `crates/crush-cast/python/cast_types.py`.
- `crush-walk-run -t` labels microseconds as `s`.

## Success criteria

- [ ] `crush-walk-run` registers the same default stdlib as `crush-run`.
- [ ] `walker` resolves sibling binaries next to its own executable; validates `-f`.
- [ ] Every binary uses clap (or equivalent) for `--help`/arg validation; `export-py`
      takes an explicit output path.
