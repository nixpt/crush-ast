#!/usr/bin/env python3
"""Publish this workspace's crates to crates.io, in dependency order.

    scripts/publish-crates.py                       # plan + verify, publish nothing
    scripts/publish-crates.py --publish             # plan + verify + publish
    scripts/publish-crates.py --publish --verified  # publish; the set was verified earlier

`--verified` lets CI verify before fetching a short-lived crates.io token
(Trusted Publishing tokens expire after 30 minutes).

Which crates: every publishable workspace member that already exists on
crates.io. A crate that has never been published is left out: crates.io
Trusted Publishing can only be configured for a crate that exists, so a new
crate's first release is a manual `cargo publish -p <crate>` (then add
Trusted Publishing for it on crates.io).

A crate that needs a never-published workspace crate (directly or through
another waiting crate) waits; everything else is published. The run then
names the crates that need a manual first publish; publish those by hand and
run again to finish.

Order: normal and build dependencies between workspace crates, dependencies
first. Dev-dependencies don't order the set: crates.io accepts path-only
dev-deps, and counting them creates false cycles. A dev-dep that carries a
version requirement (e.g. inherited via `.workspace = true`) does make a crate
wait, though: `cargo package` keeps it, so that version has to be on
crates.io or in the same publish set.

Safe to re-run: a version already on crates.io is skipped. `cargo publish`
waits for each crate to appear in the index before the next one starts.
"""

import json
import subprocess
import sys
import time
import urllib.error
import urllib.request

USER_AGENT = "crush-ast publish workflow (github.com/nixpt/crush-ast)"


def crates_io_versions(name):
    """Every version of `name` on crates.io (yanked included), or None if absent."""
    req = urllib.request.Request(
        f"https://crates.io/api/v1/crates/{name}", headers={"User-Agent": USER_AGENT}
    )
    for attempt in range(3):
        try:
            with urllib.request.urlopen(req, timeout=30) as resp:
                return {v["num"] for v in json.load(resp)["versions"]}
        except urllib.error.HTTPError as e:
            if e.code == 404:
                return None
            if attempt == 2:
                raise
        except urllib.error.URLError:
            if attempt == 2:
                raise
        time.sleep(2 * (attempt + 1))
    return None


def workspace():
    result = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        # show cargo's own reason (e.g. a missing path dependency)
        sys.stderr.write(result.stderr)
        sys.exit(f"cargo metadata failed (exit {result.returncode}); see cargo's error above")
    meta = json.loads(result.stdout)
    members = set(meta["workspace_members"])
    packages = {
        p["name"]: p
        for p in meta["packages"]
        if p["id"] in members and p.get("publish") != []
    }
    deps = {
        name: sorted(
            {
                d["name"]
                for d in p["dependencies"]
                if d["name"] in packages and d["kind"] in (None, "build")
            }
        )
        for name, p in packages.items()
    }
    # What `cargo package` must resolve: normal and build deps, plus dev-deps
    # that carry a version requirement. A path-only dev-dep (req "*") is
    # stripped from the published manifest; one with a version (e.g. via
    # `.workspace = true`) is kept, so that version must be on crates.io or
    # in the same publish set.
    needs = {
        name: sorted(
            set(deps[name])
            | {
                d["name"]
                for d in p["dependencies"]
                if d["name"] in packages and d["kind"] == "dev" and d["req"] != "*"
            }
        )
        for name, p in packages.items()
    }
    return packages, deps, needs


def topo_order(names, deps):
    order, seen = [], set()

    def visit(n):
        if n in seen:
            return
        seen.add(n)
        for d in deps[n]:
            visit(d)
        order.append(n)

    for n in sorted(names):
        visit(n)
    return order


def main():
    publish = "--publish" in sys.argv[1:]
    verified = "--verified" in sys.argv[1:]
    packages, deps, needs = workspace()
    on_registry = {name: crates_io_versions(name) for name in sorted(packages)}

    in_set = {n for n, versions in on_registry.items() if versions is not None}
    not_published = sorted(set(packages) - in_set)

    # A crate waits if it needs a never-published workspace crate, directly
    # or through another waiting crate. Everything else is published now; a
    # re-run after the manual first publishes picks up the rest. A versioned
    # dev-dep can point either way in the publish order, so iterate to a
    # fixed point instead of deciding in one ordered pass.
    pending = [
        n for n in topo_order(in_set, deps)
        if n in in_set and packages[n]["version"] not in on_registry[n]
    ]
    waiting = {}
    changed = True
    while changed:
        changed = False
        for name in pending:
            firsts = {
                d for d in needs[name]
                if d not in in_set and packages[d]["version"] not in (on_registry[d] or ())
            } | {m for d in needs[name] if d in waiting for m in waiting[d]}
            if firsts - waiting.get(name, set()):
                waiting[name] = waiting.get(name, set()) | firsts
                changed = True
    waiting = {name: sorted(waiting[name]) for name in pending if name in waiting}

    done = [
        f"{n} {packages[n]['version']}"
        for n in topo_order(in_set, deps)
        if n in in_set and packages[n]["version"] in on_registry[n]
    ]
    todo = [(n, packages[n]["version"]) for n in pending if n not in waiting]

    print(f"already on crates.io ({len(done)}): {', '.join(done) or '-'}")
    print(f"never published, left out ({len(not_published)}): {', '.join(not_published) or '-'}")
    print(f"to publish now, in order ({len(todo)}):")
    for name, version in todo:
        print(f"  {name} {version}")

    def report_waiting():
        if not waiting:
            return
        firsts = sorted({m for needs in waiting.values() for m in needs})
        print(f"\nWAITING ({len(waiting)}): these need a crate that has never been published:")
        for name, needs in waiting.items():
            print(f"  {name} needs {', '.join(needs)}")
        print("After this run, publish these once by hand (dependencies first), add Trusted Publishing")
        print("for each on crates.io, then run this again to publish the rest:")
        for m in topo_order(firsts, deps):
            if m in firsts:
                print(f"  cargo publish -p {m}")
        print(f"::warning::{len(waiting)} crate(s) wait on a first publish of {', '.join(firsts)}")

    if not todo:
        print("\nnothing to publish now")
        report_waiting()
        return

    # Package and verify the whole set before uploading anything, so a crate
    # that can't build from its tarball fails the run before its dependencies
    # are already on crates.io. Cargo verifies the selected packages together,
    # resolving each one's in-set dependencies to the local tarballs.
    if not verified:
        print("\n::group::cargo package (verify the whole set)", flush=True)
        package = ["cargo", "package", "--locked"]
        for name, _ in todo:
            package += ["-p", name]
        subprocess.run(package, check=True)
        print("::endgroup::", flush=True)

    if not publish:
        print("\nverified; plan only (pass --publish to publish)")
        report_waiting()
        return

    # Already verified above; --no-verify skips building each crate twice.
    # A crate that fails to upload (typically crates.io refusing this
    # workflow's token for it: no Trusted Publishing config for that crate)
    # doesn't stop the run. Only the crates that need it are skipped, so one
    # run reports every crate that needs attention, not just the first.
    failed, skipped, published = {}, {}, []
    for name, version in todo:
        blocked = sorted(d for d in needs[name] if d in failed or d in skipped)
        if blocked:
            skipped[name] = blocked
            print(f"\nskipping {name}: needs {', '.join(blocked)}, which did not publish", flush=True)
            continue
        print(f"\n::group::cargo publish -p {name} ({version})", flush=True)
        result = subprocess.run(
            ["cargo", "publish", "-p", name, "--locked", "--no-verify"],
            stderr=subprocess.PIPE, text=True,
        )
        sys.stderr.write(result.stderr)
        print("::endgroup::", flush=True)
        if result.returncode == 0:
            published.append(name)
        else:
            denied = "403" in result.stderr or "not valid for crate" in result.stderr
            failed[name] = "token refused" if denied else f"cargo publish exited {result.returncode}"
            print(f"::error::{name}: {failed[name]}", flush=True)
    print(f"\npublished {len(published)} crate(s)" + (f": {', '.join(published)}" if published else ""))
    report_waiting()
    if failed or skipped:
        print(f"\nFAILED ({len(failed)}):")
        for name, why in failed.items():
            print(f"  {name}: {why}")
        if skipped:
            print(f"SKIPPED ({len(skipped)}), because a crate they need did not publish:")
            for name, blocked in skipped.items():
                print(f"  {name} needs {', '.join(blocked)}")
        refused = [n for n, why in failed.items() if why == "token refused"]
        if refused:
            print("crates.io refused this workflow's token for these. Add Trusted Publishing for each")
            print("(crates.io > crate > Settings > Trusted Publishing: GitHub, repository nixpt/crush-ast,")
            print("workflow publish.yml), then run this again: it skips what is already published.")
            for n in refused:
                print(f"  https://crates.io/crates/{n}/settings")
        sys.exit(1)


if __name__ == "__main__":
    main()
