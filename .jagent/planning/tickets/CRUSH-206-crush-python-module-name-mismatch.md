# CRUSH-206 — crush-python: `PyInit_crush` vs lib name `crush_python`; `cast_version()` hard-coded; error handling

| Field | Value |
|-------|-------|
| **ID** | CRUSH-206 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M4 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | XS |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

- `#[pymodule] fn crush` exports `PyInit_crush`, but `[lib] name = "crush_python"` and
  `pyproject.toml` has no `module-name`. Importing as `crush_python` fails
  ("does not define module export function"); `maturin develop` very likely produces
  an unimportable module (not verified — maturin unavailable in the sandbox).
- `cast_version()` returns hard-coded `"0.2"` while `crush_cast::CAST_VERSION` is `"0.1"`
  (`crush-python/src/lib.rs:~52`, `crush-cast/src/pack.rs:~31`); README example says 0.2.
- `validate_cast` raises instead of returning `False`; `run_casm` returns a debug
  string instead of raising (see CRUSH-205).
- `crush-vm-py` hard-codes `version = "0.3.0"` against a 0.4.0 workspace.

## Success criteria

- [ ] `maturin develop && python -c "import crush"` works; add a CI smoke test.
- [ ] `cast_version()` reads the constant; errors raise Python exceptions consistently.
