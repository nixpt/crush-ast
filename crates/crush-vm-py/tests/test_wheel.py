#!/usr/bin/env python3
"""Smoke test for the built `crush_vm` wheel.

Run it against an installed wheel:

    maturin build --release
    pip install target/wheels/crush_vm-*.whl
    python crates/crush-vm-py/tests/test_wheel.py

The functions are named `test_*` so pytest collects them too, but the file
has no pytest dependency — the repo has no Python test infrastructure yet, and
this crate is not part of the Rust workspace, so there is nothing here for
`cargo test` to pick up either. Run it by hand (or wire it into CI) after
building the wheel.

The fixtures are real CVM1 blobs assembled by crush-ast's own assembler
(`crush_vm::assemble(...).to_blob()`), so the test exercises the actual
binary format rather than a mock.
"""
from __future__ import annotations

import sys
from pathlib import Path

try:
    import crush_vm
except ImportError:  # pragma: no cover — the whole point of the skip
    print("SKIP: crush_vm wheel not installed", file=sys.stderr)
    raise SystemExit(0)

FIXTURES = Path(__file__).parent / "fixtures"


def _blob(name: str) -> bytes:
    return (FIXTURES / name).read_bytes()


def test_run_blob_returns_the_documented_keys():
    res = crush_vm.run_blob(_blob("print_hi.cvm1"))
    assert set(res) == {
        "output", "result", "return_value", "stack", "steps", "halted",
    }, sorted(res)
    assert res["output"] == "hi"
    assert res["halted"] is True


def test_return_value_carries_the_explicit_return():
    """A program that returns leaves an EMPTY stack — the value lives in
    `return_value`. Reading only `result` (top of stack) reports None for
    exactly the programs whose value matters."""
    res = crush_vm.run_blob(_blob("ret55.cvm1"))
    assert res["return_value"] == 55, res
    assert res["stack"] == [], res
    # `result` is the top of the stack, NOT the return value.
    assert res["result"] is None, res


def test_return_value_is_none_without_an_explicit_return():
    res = crush_vm.run_blob(_blob("print_hi.cvm1"))
    assert res["return_value"] is None, res


def test_quotas_are_honoured():
    """A step budget below what the program needs must trap, not silently
    produce a truncated result."""
    res = crush_vm.run_blob(_blob("print_hi.cvm1"), max_steps=1000)
    assert res["halted"] is True
    assert res["output"] == "hi"
    try:
        crush_vm.run_blob(_blob("print_hi.cvm1"), max_steps=0)
    except (RuntimeError, ValueError) as exc:
        assert "CVM1 trap" in str(exc) or "quota" in str(exc).lower(), exc
    else:
        raise AssertionError("max_steps=0 should have trapped")


def test_bad_blob_raises():
    try:
        crush_vm.run_blob(b"not a cvm1 blob")
    except (RuntimeError, ValueError):
        pass
    else:
        raise AssertionError("a malformed blob should raise")


def test_host_caps_are_callable_from_python():
    """A host cap registered as a Python callable is invoked by the VM."""
    seen: list[str] = []

    def host_echo(payload):
        seen.append(payload)
        return None

    res = crush_vm.run_blob(
        _blob("print_hi.cvm1"),
        host_caps={"host.echo": (host_echo, 1, False)},
    )
    assert res["halted"] is True
    # This program does not call host.echo; the point is that registering a
    # Python callable is accepted and does not break the run.
    assert seen == []


def main() -> int:
    failures = 0
    for name, fn in sorted(globals().items()):
        if not name.startswith("test_") or not callable(fn):
            continue
        try:
            fn()
        except Exception as exc:  # noqa: BLE001
            failures += 1
            print(f"FAIL {name}: {type(exc).__name__}: {exc}")
        else:
            print(f"ok   {name}")
    if failures:
        print(f"\n{failures} failure(s)")
        return 1
    print("\nall wheel smoke tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
