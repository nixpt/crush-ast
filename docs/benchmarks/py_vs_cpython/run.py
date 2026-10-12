#!/usr/bin/env python3
"""Python → crush AOT vs CPython: output check and timing.

  export CARGO_TARGET_DIR=$PWD/target   # one dir for both (repo root)
  cargo build --release -p crush-aot --bin crush-aot-runner
  cargo build --release --manifest-path docs/benchmarks/py_vs_cpython/pyaot/Cargo.toml
  python3 run.py check|perf  [--target DIR]
"""
import argparse
import json
import os
import statistics
import subprocess
import sys
import time

_ap = argparse.ArgumentParser()
_ap.add_argument("mode", choices=["check", "perf"])
_ap.add_argument("--target")
_args = _ap.parse_args()
B = os.path.dirname(os.path.abspath(__file__))
T = _args.target or os.path.join(B, "../../../target/release")
PYAOT = os.environ.get("PYAOT") or next(
    (
        c
        for c in (f"{T}/pyaot", os.path.join(B, "pyaot/target/release/pyaot"))
        if os.path.exists(c)
    ),
    f"{T}/pyaot",
)
RUNNER = f"{T}/crush-aot-runner"
PYTHON = os.environ.get("PYTHON", "python3")


def program(name, n):
    src = open(f"{B}/py/{name}.py").read().replace("__N__", str(n))
    path = f"{B}/gen/{name}_{n}.py"
    os.makedirs(f"{B}/gen", exist_ok=True)
    open(path, "w").write(src)
    return path


def aot_stdout(so):
    r = subprocess.run([RUNNER, so], capture_output=True, text=True)
    if r.returncode:
        raise RuntimeError(r.stderr.strip()[:300])
    lines = r.stdout.rstrip("\n").split("\n")
    return "\n".join(lines[:-1]) + "\n" if len(lines) > 1 else ""


def compile_py(py, backend):
    r = subprocess.run([PYAOT, py, backend], capture_output=True, text=True)
    if r.returncode:
        raise RuntimeError(r.stderr.strip()[:400] or r.stdout.strip()[:400])
    return r.stdout.strip()


mode = _args.mode
if mode == "check":
    for name, n in [("fib", 20), ("loop", 1000), ("sieve", 1000), ("strings", 1000)]:
        py = program(name, n)
        cpy = subprocess.run([PYTHON, py], capture_output=True, text=True).stdout
        for be in ["rust", "gcc", "clang"]:
            try:
                so = compile_py(py, be)
                got = aot_stdout(so)
                ok = got == cpy
                print(
                    f"{name:8} {be:6} {'OK ' if ok else 'DIFF'} "
                    f"cpy={cpy.strip()!r} aot={got.strip()!r} so={os.path.getsize(so)//1024}KiB"
                )
            except Exception as e:
                print(f"{name:8} {be:6} FAIL {e}")


def timed(cmd, runs=7):
    walls, rss, out = [], [], None
    for _ in range(runs):
        t0 = time.perf_counter()
        p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        stdout = p.stdout.read()
        _, status, ru = os.wait4(p.pid, 0)
        walls.append(time.perf_counter() - t0)
        rss.append(ru.ru_maxrss / 1024)
        out = stdout
    return statistics.median(walls) * 1000, statistics.median(rss), out


if mode == "perf":
    rows = []
    py_bin = os.path.realpath(
        subprocess.run(["which", PYTHON], capture_output=True, text=True).stdout.strip()
        or "/usr/bin/python3"
    )
    for name, small, big in [
        ("fib", 5, 30),
        ("loop", 10, 20_000_000),
        ("sieve", 10, 2_000_000),
    ]:
        for n in (small, big):
            py = program(name, n)
            ms, mb, out = timed([PYTHON, py])
            rows.append((name, n, "cpython", ms, mb, os.path.getsize(py_bin)))
            ref = out
            for be in ["rust", "gcc", "clang"]:
                so = compile_py(py, be)
                ms, mb, out = timed([RUNNER, so])
                ok = (
                    out.decode().rstrip("\n").split("\n")[:-1]
                    == ref.decode().rstrip("\n").split("\n")
                )
                rows.append(
                    (
                        name,
                        n,
                        f"aot-{be}" + ("" if ok else " (DIFF)"),
                        ms,
                        mb,
                        os.path.getsize(so) + os.path.getsize(RUNNER),
                    )
                )
    print(
        f"{'program':8} {'n':>10} {'engine':12} {'wall ms':>9} {'peak MiB':>9} {'artifact':>10}"
    )
    for r in rows:
        print(
            f"{r[0]:8} {r[1]:>10} {r[2]:12} {r[3]:9.1f} {r[4]:9.1f} {r[5]/1048576:9.2f}M"
        )
    json.dump(rows, open(f"{B}/results.json", "w"))
