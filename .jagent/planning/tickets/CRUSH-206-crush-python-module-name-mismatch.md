# CRUSH-206 — crush-python: `PyInit_crush` vs lib name `crush_python`; `cast_version()` hard-coded; error handling

| Field | Value |
|-------|-------|
| **ID** | CRUSH-206 |
| **Priority** | P2 |
| **Status** | In review |
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

- [x] `maturin develop && python -c "import crush"` works; add a CI smoke test.
- [x] `cast_version()` reads the constant; errors raise Python exceptions consistently.

## Resolution (2026-10-31, `agent/claude/CRUSH-206`)

**Root cause.** `#[pymodule] fn crush` emits `PyInit_crush`, but maturin derived the
expected init symbol from `[lib] name = "crush_python"` because `pyproject.toml` had no
`module-name`. Import failed with `ImportError: dynamic module does not define module export function (PyInit_crush_python)`.

**Before** (`maturin develop --release`, then `python -I -c "import crush_python; ..."`):
```
maturin: Warning: Couldn't find the symbol `PyInit_crush_python` ...
ImportError: dynamic module does not define module export function (PyInit_crush_python)
```
(`import crush` -> `ModuleNotFoundError: No module named 'crush'`)

**After** (same venv, final code):
```
$ python -I -c "import crush; print(crush.cast_version())"
0.1.0
```
No PyInit warning from maturin.

**Changes**
- `crates/crush-python/pyproject.toml` `[tool.maturin]`: `module-name = "crush"` (the fix).
- `crates/crush-python/src/lib.rs` `cast_version()`: already returns `crush_cast::CAST_VERSION`
  (`0.1.0`) on main; the hard-coded `"0.2"` is gone. Covered by `test_cast_version` and the smoke test.
- `crates/crush-python/src/lib.rs` `validate_cast`: returns `bool`. True when the input parses as
  CAST and `Program::check_version()` passes; False for malformed JSON or an incompatible
  `cast_version` major. Never raises. Judgment call: the ticket asks for False; `parse_cast` still raises `ValueError`.
- `crates/crush-python/src/lib.rs` `run_casm`: `FastYield::Error` raises `RuntimeError("VM Execution Error: ...")`
  (CRUSH-205 class). A completed run still returns the yield-state debug string (no result format specified).
- `crates/crush-python/src/lib.rs` `parse_cson`: redundant-closure lint fixed so `clippy -D warnings` passes (pre-existing, one line).
- `crates/crush-python/src/lib.rs`: `rustfmt --edition 2024` applied (reflows `parse_cson`/`run_casm` wrapping only).
- `crates/crush-python/README.md`: examples use `"0.1.0"`; adds the `python -I -c "import crush"` check.
- `crates/crush-python/tests/test_import.py` (new): import, version, `validate_cast` verdicts, `parse_cast` raises.
- `.github/workflows/ci.yml` (`test-rest` job): new step builds a wheel with maturin, installs it into a clean venv, runs the smoke test.

**Tests**
- `cargo test -p crush-python`: `test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` (new: `test_validate_cast_invalid_returns_false`, `test_validate_cast_incompatible_major_returns_false`, `test_run_casm_runtime_fault_raises`).
- `python -I -m unittest discover -s crates/crush-python/tests`: `Ran 3 tests ... OK`.
- `cargo clippy -p crush-python --all-targets --no-deps -- -D warnings`: clean.

**Not in this ticket** (filed as follow-ups)
- `crates/crush-vm-py` hard-codes `version = "0.3.0"` (excluded from the workspace).
- `crates/crush-python/pyproject.toml` `version = "0.2.0"` lags the crate (0.10.0).

