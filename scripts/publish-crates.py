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

The plan stops before publishing anything if a crate in the set depends on a
workspace crate that is not on crates.io at the version this tree needs; it
names the crates that need that manual first publish.

Order: normal and build dependencies between workspace crates, dependencies
first. Dev-dependencies are ignored: crates.io accepts path-only dev-deps,
and counting them creates false cycles.

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
    meta = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    )
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
    return packages, deps


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
    packages, deps = workspace()
    on_registry = {name: crates_io_versions(name) for name in sorted(packages)}

    in_set = {n for n, versions in on_registry.items() if versions is not None}
    not_published = sorted(set(packages) - in_set)

    todo, done, blocked = [], [], []
    for name in (n for n in topo_order(in_set, deps) if n in in_set):
        version = packages[name]["version"]
        if version in on_registry[name]:
            done.append(f"{name} {version}")
            continue
        missing = [
            d for d in deps[name]
            if d not in in_set and packages[d]["version"] not in (on_registry[d] or ())
        ]
        if missing:
            blocked.append((name, missing))
        todo.append((name, version))

    print(f"already on crates.io ({len(done)}): {', '.join(done) or '-'}")
    print(f"never published, left out ({len(not_published)}): {', '.join(not_published) or '-'}")
    print(f"to publish, in order ({len(todo)}):")
    for name, version in todo:
        print(f"  {name} {version}")

    if blocked:
        print("\nBLOCKED: these depend on workspace crates that are not on crates.io yet:")
        for name, missing in blocked:
            print(f"  {name} needs {', '.join(missing)}")
        firsts = sorted({m for _, missing in blocked for m in missing})
        print("Publish each of these once by hand, then add Trusted Publishing for it on crates.io:")
        for m in topo_order(firsts, deps):
            if m in firsts:
                print(f"  cargo publish -p {m}")
        sys.exit(1)

    if not todo:
        print("\nnothing to publish")
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
        return

    # Already verified above; --no-verify skips building each crate twice.
    for name, version in todo:
        print(f"\n::group::cargo publish -p {name} ({version})", flush=True)
        subprocess.run(["cargo", "publish", "-p", name, "--locked", "--no-verify"], check=True)
        print("::endgroup::", flush=True)
    print(f"\npublished {len(todo)} crate(s)")


if __name__ == "__main__":
    main()
