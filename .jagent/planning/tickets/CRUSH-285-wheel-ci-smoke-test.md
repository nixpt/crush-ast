# CRUSH-285 — the `crush_vm` wheel has no CI job: it is never built, never imported, never tested

> Numbered CRUSH-285 by kai (2026-10-21): first filed as CRUSH-275, which PR #281 (Crush 1.0 release criteria)
> had already claimed.

| Field | Value |
|---|---|
| **ID** | CRUSH-285 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M6 |
| **Assignee** | — |
| **Dependencies** | — |
| **Estimated effort** | S |
| **Filed by** | nixp — 2026-10-20, while shipping the abi3 + `return_value` fix to `crates/crush-vm-py` |

## Problem

`crates/crush-vm-py` — the crate that builds the `crush_vm` Python wheel — is
**excluded from the crush-ast workspace** (`exclude = ["crates/crush-web",
"crates/crush-vm-py"]` in the root `Cargo.toml`). That is deliberate and
correct: it keeps a `cdylib` out of the main build graph, and it means
`cargo test --workspace` never builds pyo3.

The consequence nobody has paid for yet: **no CI job builds the wheel either.**
The `ci.yml` jobs are `check`, `msrv`, `test-core`, `test-aot*`, `test-rest`,
`test-languages`, `test-sdk`, `test-workspace`, `lint`, `build-release` —
none of them touch `crates/crush-vm-py` or `crates/crush-python`. So the
wheel is:

- never compiled by CI, so a pyo3 or `crush-vm` API change breaks it silently
- never imported by CI, so a broken module name or ABI tag goes unnoticed
- never executed by CI, so a behavioural change in `run_blob` goes unnoticed

This is the same class of gap CRUSH-206 flags for `crush-python` ("maturin
develop && python -c 'import crush' works; add a CI smoke test"), and it is
the reason the `return_value` omission survived: the wheel was correct-by-
inspection, not by test.

The wheel is not a toy — it is the **canonical CVM1 backend** for the chroma
project, which prefers it by default (`backend="auto"` → `crush_vm.run_blob`)
because it is the same VM crush-ast and crush-ptx execute.

## Reproduction

```bash
# Nothing in CI builds it:
grep -l "crush-vm-py\|maturin" .github/workflows/*.yml    # → no matches
```

## Technical approach

Add a CI job that builds the wheel and runs the smoke test:

```yaml
wheel:
  name: Wheel (crush-vm-py)
  steps:
    - uses: actions/checkout@v4
    - uses: actions/setup-python@v5
      with: { python-version: '3.14' }   # the version pyo3 0.23 could NOT build against before abi3
    - run: pip install maturin
    - run: cd crates/crush-vm-py && maturin build --release
    - run: pip install crates/crush-vm-py/target/wheels/crush_vm-*.whl
    - run: python crates/crush-vm-py/tests/test_wheel.py
```

`crates/crush-vm-py/tests/test_wheel.py` already exists (added with the
abi3 + `return_value` fix) and needs no pytest — it is a plain script with
`test_*` functions and a `__main__` runner, so it works standalone today and
under pytest if the repo ever adopts it.

Decisions to make while wiring it up:

- **Which Python version(s).** 3.14 is the interesting one (it is what pyo3
  0.23 refused to build against before the abi3 switch, so it is the
  regression guard). A 3.11 or 3.12 leg would additionally prove the
  stable-ABI floor. `abi3-py311` means one wheel serves all of them.
- **Whether the same job should cover `crush-python`** (also a wheel, also
  unbuilt by CI). It has its own open problems in CRUSH-206
  (`PyInit_crush` vs `[lib] name = "crush_python"`, hard-coded
  `cast_version()`), so it may be better as its own job.
- **Whether the wheel should be published to PyPI.** It currently is not, so
  `pip install crush-vm` fails and every consumer must build it from source.
  Publishing would make the dependency a normal resolvable one — but it is a
  release-policy decision, not a CI decision.

## Success criteria

- [ ] CI builds the `crush_vm` wheel on Python 3.14 and installs it.
- [ ] `crates/crush-vm-py/tests/test_wheel.py` runs in CI and is green.
- [ ] The job fails if the wheel's `run_blob` contract changes shape (the
      test asserts the exact key set, so a renamed/removed key is a failure).
- [ ] `crates/crush-vm-py/README.md` documents the CI job alongside the manual
      build recipe.
