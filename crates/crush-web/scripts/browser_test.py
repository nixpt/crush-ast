#!/usr/bin/env python3
"""Play blackjack_interactive.crush in headless Chromium via crush-web.

usage: browser_test.py <site-dir>   (a dir holding index.html, pkg/, fixtures/;
                                     scripts/browser-test.sh stages one)

Serves <site-dir> on a free localhost port, loads index.html?auto (one scripted
round through `Session`, and the same answers through `execute_with`), prints
both transcripts, and exits non-zero unless the session paused for input before
every answer, finished after the last one, and printed exactly what the
one-shot run printed. The server is a child process and is always terminated.
"""
import json
import socket
import subprocess
import sys
import time

from playwright.sync_api import sync_playwright


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def main(site):
    port = free_port()
    server = subprocess.Popen(
        [sys.executable, "-m", "http.server", str(port), "--bind", "127.0.0.1", "--directory", site],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        time.sleep(0.5)
        with sync_playwright() as p:
            browser = p.chromium.launch()
            page = browser.new_page()
            errors = []
            page.on("pageerror", lambda e: errors.append(f"page error: {e}"))
            page.goto(f"http://127.0.0.1:{port}/index.html?auto")
            page.wait_for_function("window.__crushResults !== undefined", timeout=30000)
            data = page.evaluate("window.__crushResults")
            ua = page.evaluate("navigator.userAgent")
            browser.close()
    finally:
        server.terminate()
        server.wait(timeout=10)

    reports = data["session"]["reports"]
    transcript = data["session"]["transcript"]
    one_shot = data["executeWith"]
    print(f"# {ua}")
    for answer, report in zip([None] + data["answers"], reports):
        if answer is not None:
            print(f"> {answer}")
        print(report["output"], end="")
        print(f"[{report['status']}, steps={report['steps']}]")
    print("# execute_with:", json.dumps({k: v for k, v in one_shot.items() if k != "output"}))
    print(one_shot["output"], end="")

    failures = list(errors)
    statuses = [r["status"] for r in reports]
    want = ["need_input"] * len(data["answers"]) + ["done"]
    if statuses != want:
        failures.append(f"session statuses {statuses}, expected {want}")
    if "".join(r["output"] for r in reports) != transcript:
        failures.append("per-call outputs don't add up to the transcript")
    if not one_shot["ok"]:
        failures.append(f"execute_with failed: {one_shot.get('error')}")
    if one_shot["output"] != transcript:
        failures.append("execute_with output differs from the session transcript")
    for needle in ("Dealing (bet: $10)...", "Drew: ", "You stand at ", "You leave the table with $"):
        if needle not in transcript:
            failures.append(f"transcript lacks {needle!r}")
    for f in failures:
        print("FAIL", f, file=sys.stderr)
    print("PASS" if not failures else f"{len(failures)} failure(s)", file=sys.stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
