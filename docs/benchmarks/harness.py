#!/usr/bin/env python3
"""Crush Walker Pipeline Benchmark Harness.

Measures three stages for Python and JavaScript source translation,
plus native C (gcc -O3) compiled baselines for the algorithm benchmarks.

Stages:
  1. Walk:  source → CAST JSON  (via python_walker / js_walker)
  2. Full:  source → CAST → CASM → FastVM  (via crush-walk-run)
  3. Native: native interpreter/compiled execution

Output modes:
  - Terminal table (default)
  - Markdown report (--markdown)
  - CSV (--csv)

Usage:
  python3 harness.py [--runs N] [--warmup N] [--markdown] [--csv] [--bench ALGO]
"""

import subprocess
import time
import sys
import os
import statistics
import argparse

# ── Configuration ───────────────────────────────────────────────────────────

WORKSPACE = os.path.dirname(os.path.abspath(__file__))
TARGET_DIR = os.environ.get("CRUSH_BUILD_DIR", "/tmp/crush-build")
BENCH_DIR = os.path.join(WORKSPACE, "docs", "benchmarks")

# Binary paths
PY_WALKER = os.path.join(TARGET_DIR, "debug", "python_walker")
JS_WALKER = os.path.join(TARGET_DIR, "debug", "js_walker")
WALK_RUN  = os.path.join(TARGET_DIR, "debug", "crush-walk-run")

# Available algorithm suites
ALGORITHMS = {
    "simple": {
        "name": "Simple arithmetic (3 ops)",
        "py": "walk_simple.py", "js": "walk_simple.js",
    },
    "compute": {
        "name": "Compute chain (11 ops)",
        "py": "walk_compute.py", "js": "walk_compute.js",
    },
    "nqueens": {
        "name": "N-Queens backtracking (n=12)",
        "py": "nqueens.py", "js": "nqueens.js", "c": "nqueens.c",
        "c_binary": "nqueens_c_exe",
    },
    "sieve": {
        "name": "Sieve of Eratosthenes (n=1M)",
        "py": "sieve.py", "js": "sieve.js", "c": "sieve.c",
        "c_binary": "sieve_c_exe",
    },
    "mergesort": {
        "name": "Merge sort (n=5000)",
        "py": "mergesort.py", "js": "mergesort.js", "c": "mergesort.c",
        "c_binary": "mergesort_c_exe",
    },
}


# ── Helpers ─────────────────────────────────────────────────────────────────

def run_cmd(cmd_args, timeout=60):
    """Run a command, return (elapsed_ms, stdout, stderr, returncode)."""
    start = time.perf_counter_ns()
    try:
        res = subprocess.run(cmd_args, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return (timeout * 1000, "", "TIMEOUT", -1)
    elapsed_ns = time.perf_counter_ns() - start
    return (elapsed_ns / 1_000_000, res.stdout.strip(), res.stderr.strip(), res.returncode)


def measure(cmd_args, runs=10, warmup=2, label=""):
    """Run a command N times, return (mean_ms, stdev_ms) or (None, None)."""
    times = []
    for i in range(warmup + runs):
        ms, stdout, stderr, rc = run_cmd(cmd_args)
        if rc != 0 and i >= warmup:
            print(f"  [{label}] ERROR (run {i}): {stderr[:120]}", file=sys.stderr)
            return (None, None)
        if i >= warmup:
            times.append(ms)
    if not times:
        return (None, None)
    mean = statistics.mean(times)
    stdev = statistics.stdev(times) if len(times) > 1 else 0.0
    return (mean, stdev)


def ensure_built():
    """Build required binaries if they don't exist."""
    need_build = False
    for b in [PY_WALKER, JS_WALKER, WALK_RUN]:
        if not os.path.exists(b):
            need_build = True
            break
    if need_build:
        print("[*] Building walker binaries (one-time, ~2 min)...")
        subprocess.run(
            ["cargo", "build", "--bin", "crush-walk-run",
             "--bin", "python_walker", "--bin", "js_walker"],
            cwd=WORKSPACE,
            env={**os.environ, "CARGO_TARGET_DIR": TARGET_DIR},
            check=True,
        )
        print("[*] Build complete.\n")


def compile_c_baselines(benches):
    """Compile C source files with gcc -O3."""
    for key, algo in benches.items():
        c_file = algo.get("c")
        if not c_file:
            continue
        src_path = os.path.join(BENCH_DIR, c_file)
        binary_name = algo.get("c_binary", c_file.replace(".c", "_c_exe"))
        binary_path = os.path.join(BENCH_DIR, binary_name)
        if not os.path.exists(src_path):
            continue
        if os.path.exists(binary_path) and os.path.getmtime(binary_path) >= os.path.getmtime(src_path):
            continue  # already up-to-date
        print(f"[*] Compiling {c_file} → {binary_name}...")
        subprocess.run(
            ["gcc", "-O3", "-o", binary_path, src_path, "-lm"],
            check=True,
        )
        os.chmod(binary_path, 0o755)


def ensure_test_files():
    """Create minimal test files."""
    os.makedirs(BENCH_DIR, exist_ok=True)

    files = {
        "walk_simple.py": "x = 40 + 2\ny = 10 + x\nz = y * 2\n",
        "walk_simple.js": "let x = 40 + 2;\nlet y = 10 + x;\nlet z = y * 2;\n",
        "walk_compute.py": (
            "a = 100\nb = a + 50\nc = b * 3\nd = c - 200\n"
            "e = int(d / 5)\nf = e + 77\ng = f * 2\nh = g - 100\n"
            "i = h + 1\nj = i * 3\n"
        ),
        "walk_compute.js": (
            "let a = 100;\nlet b = a + 50;\nlet c = b * 3;\n"
            "let d = c - 200;\nlet e = Math.floor(d / 5);\n"
            "let f = e + 77;\nlet g = f * 2;\nlet h = g - 100;\n"
            "let i = h + 1;\nlet j = i * 3;\n"
        ),
    }
    for name, content in files.items():
        path = os.path.join(BENCH_DIR, name)
        if not os.path.exists(path):
            with open(path, "w") as f:
                f.write(content)


# ── Report generation ───────────────────────────────────────────────────────

def fmt_ms(mean, stdev):
    if mean is None:
        return "FAILED"
    return f"{mean:.2f} ms  ±{stdev:.1f}"


class BenchRow:
    def __init__(self, algo_name, language, tier, mean, stdev):
        self.algo = algo_name
        self.lang = language
        self.tier = tier
        self.mean = mean
        self.stdev = stdev


def generate_markdown(rows, runs, warmup):
    """Generate a Markdown benchmark report."""
    lines = []
    lines.append("# Crush Walker Pipeline Benchmark Report")
    lines.append("")
    lines.append(f"**Parameters:** {runs} measurement runs, {warmup} warmup runs")
    lines.append("")
    lines.append(f"**Generated:** {time.strftime('%Y-%m-%d %H:%M:%S')}")
    lines.append("")

    current_algo = None
    for row in rows:
        if row.algo != current_algo:
            current_algo = row.algo
            lines.append(f"## {row.algo}")
            lines.append("")
            lines.append("| Language | Tier | Time |")
            lines.append("|----------|------|------|")
        lines.append(f"| {row.lang} | {row.tier} | {fmt_ms(row.mean, row.stdev)} |")

    lines.append("")
    lines.append("### Tiers")
    lines.append("- **Native** = python3 / node / gcc -O3 binary")
    lines.append("- **Walker** = source → CAST JSON (parser + lowering)")
    lines.append("- **Full** = walk + CAST→CASM compile + FastVM execution")
    return "\n".join(lines)


def generate_csv(rows):
    """Generate CSV output."""
    lines = ["algorithm,language,tier,mean_ms,stdev_ms"]
    for row in rows:
        mean = f"{row.mean:.2f}" if row.mean is not None else "FAILED"
        stdev = f"{row.stdev:.1f}" if row.stdev is not None else "0"
        lines.append(f'"{row.algo}",{row.lang},{row.tier},{mean},{stdev}')
    return "\n".join(lines)


# ── Main ──��─────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description="Crush Walker Pipeline Benchmark")
    parser.add_argument("--runs", type=int, default=20, help="Measurement runs (default: 20)")
    parser.add_argument("--warmup", type=int, default=3, help="Warmup runs (default: 3)")
    parser.add_argument("--skip-build", action="store_true", help="Skip building binaries")
    parser.add_argument("--markdown", action="store_true", help="Output Markdown report")
    parser.add_argument("--csv", action="store_true", help="Output CSV")
    parser.add_argument("--bench", choices=list(ALGORITHMS.keys()),
                        help="Run only a specific benchmark")
    args = parser.parse_args()

    if not args.skip_build:
        ensure_built()
    ensure_test_files()

    # Select benchmarks
    if args.bench:
        selected = {args.bench: ALGORITHMS[args.bench]}
    else:
        selected = ALGORITHMS

    compile_c_baselines({k: v for k, v in selected.items() if v.get("c")})

    all_rows = []

    print(f"{'='*80}")
    print(f"  Crush Walker Pipeline Benchmark  (runs={args.runs}, warmup={args.warmup})")
    print(f"{'='*80}\n")

    for key, algo in selected.items():
        name = algo["name"]
        py_file = os.path.join(BENCH_DIR, algo.get("py", ""))
        js_file = os.path.join(BENCH_DIR, algo.get("js", ""))
        c_binary = os.path.join(BENCH_DIR, algo.get("c_binary", ""))

        print(f"{'��'*70}")
        print(f"  {name}")
        print(f"{'─'*70}")

        # ── Python ──────────────────────────────────────────────────
        if os.path.exists(py_file):
            print(f"\n  Python:")
            mean, stdev = measure(["python3", py_file], args.runs, args.warmup, "py-nat")
            print(f"    Native:  {fmt_ms(mean, stdev)}")
            all_rows.append(BenchRow(name, "Python", "Native", mean, stdev))

            mean, stdev = measure([PY_WALKER, py_file], args.runs, args.warmup, "py-walk")
            print(f"    Walker:  {fmt_ms(mean, stdev)}")
            all_rows.append(BenchRow(name, "Python", "Walker (src→CAST)", mean, stdev))

            mean, stdev = measure([WALK_RUN, "-t", py_file], args.runs, args.warmup, "py-full")
            print(f"    Full:    {fmt_ms(mean, stdev)}")
            all_rows.append(BenchRow(name, "Python", "Full (→FastVM)", mean, stdev))

        # ── JavaScript ─��────────��────────────────────────────────────
        if os.path.exists(js_file):
            print(f"\n  JavaScript:")
            mean, stdev = measure(["node", js_file], args.runs, args.warmup, "js-nat")
            print(f"    Native:  {fmt_ms(mean, stdev)}")
            all_rows.append(BenchRow(name, "JavaScript", "Native", mean, stdev))

            mean, stdev = measure([JS_WALKER, js_file], args.runs, args.warmup, "js-walk")
            print(f"    Walker:  {fmt_ms(mean, stdev)}")
            all_rows.append(BenchRow(name, "JavaScript", "Walker (src→CAST)", mean, stdev))

            mean, stdev = measure([WALK_RUN, "-t", js_file], args.runs, args.warmup, "js-full")
            print(f"    Full:    {fmt_ms(mean, stdev)}")
            all_rows.append(BenchRow(name, "JavaScript", "Full (→FastVM)", mean, stdev))

        # ── C (native compiled) ──────────────────────────────────────
        if c_binary and os.path.exists(c_binary) and os.path.isfile(c_binary):
            print(f"\n  C (gcc -O3):")
            mean, stdev = measure([c_binary], args.runs, args.warmup, "c-nat")
            print(f"    Native:  {fmt_ms(mean, stdev)}")
            all_rows.append(BenchRow(name, "C", "Native (gcc -O3)", mean, stdev))

    # ── Output formats ───────────────────────────────────────────────
    if args.markdown:
        print("\n\n" + generate_markdown(all_rows, args.runs, args.warmup))
    if args.csv:
        print("\n" + generate_csv(all_rows))

    print(f"\n{'='*80}")
    print("  Notes:")
    print("  - 'Walker'  = parser + CAST lowering (no VM)")
    print("  - 'Full'    = walk + compile + FastVM execution")
    print("  - 'Native'  = direct python3/node/gcc-binary execution")
    print("  - Process startup dominates for trivial programs (30-60ms)")
    print(f"{'='*80}\n")


if __name__ == "__main__":
    main()
