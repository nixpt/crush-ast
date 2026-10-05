# CRUSH-148 — format the workspace once, then gate it in CI (`cargo fmt --check`)

| Field | Value |
|-------|--------|
| **Status** | open |
| **Milestone** | — |
| **Size** | S |
| **Owner** | — |
| **Created** | 2026-10-05 |
| **Updated** | 2026-10-05 |

## Problem

crush-ast has no formatting gate. CI (`.github/workflows/ci.yml`) runs check, tests, clippy, a release build and
feature gates, but never `cargo fmt --check`. Formatting drift has therefore accumulated, and it keeps costing time:

- **Measured on `main` `fee1888` (2026-10-05):** 202 of 319 tracked `.rs` files are not rustfmt-clean
  (`rustfmt 1.9.0-stable --edition 2024 --check`, per file).
- **CRUSH-147:** someone ran a stray `cargo fmt --all` in the shared checkout on 2026-09-17 and never committed it.
  It sat as 195 dirty files for 18 days and looked like lost feature work until a salvage and triage proved
  otherwise (see `tickets/CRUSH-147-shared-checkout-salvage.md`).
- `.dejavue/handoff.md` already records "workspace-wide rustfmt remains blocked by unrelated drift". Any horse that
  runs fmt on its change reformats unrelated files and inflates its diff.

**The toolchain pins disagree.** `rust-toolchain.toml` says `channel = "stable"` (with rustfmt + clippy), while
every CI job pins `dtolnay/rust-toolchain` to `toolchain: "1.85"`. rustfmt output differs between versions: the
CRUSH-147 salvage showed 5 files formatted differently by the 2026-09-17 rustfmt and today's 1.9.0 (import sort
order under the 2024 style edition, comment alignment, a trailing `return …;`). A gate run on 1.85 while
developers format on current stable would fail on code that was formatted correctly locally.

## Fix

- [ ] **Pick one rustfmt.** Either pin `rust-toolchain.toml` to a specific version and make CI read it (drop the
      per-job `toolchain: "1.85"`, or set both to the same version), or keep CI on 1.85 for build/test and run the
      fmt job on the toolchain file's version. Add a `rustfmt.toml` with `style_edition = "2024"` so the style
      doesn't drift with the edition default. Record the choice in `.dejavue/decisions.md`.
- [ ] **One mechanical commit:** `cargo fmt --all` from a fresh worktree on current `main`, and nothing else in
      that commit. Don't port the CRUSH-147 salvage commit: it's on an old base and 49 of its paths conflict with
      main. Regenerating is one command. Put the commit's SHA in `.git-blame-ignore-revs` so `git blame` skips it.
- [ ] **The gate:** a `Format` job in `ci.yml` running `cargo fmt --all --check` (fast; no buckets checkout or cache
      needed if fmt doesn't resolve path deps for it. Confirm, since `cargo fmt` runs `cargo metadata`).
- [ ] Land it in a **quiet window**. The mechanical commit touches about 200 files and will conflict with every open
      branch. Announce it on #general, and tell open-branch owners to rebase and re-run fmt rather than resolve
      conflicts by hand. (#58 is open as of this writing.)

## Found while filing

**`cargo fmt` fails inside an in-repo worktree** (`.jagent/worktrees/<agent>`, SQ-204 layout):
`cargo metadata` errors with *"current package believes it's in a workspace when it's not: current
.jagent/worktrees/buckets/Cargo.toml, workspace /workspace/projects/crush-ast/Cargo.toml"*. The sibling link
`.jagent/worktrees/buckets -> ../../../buckets` places `buckets` inside crush-ast's own directory tree, so cargo
walks up and finds the main checkout's workspace manifest. The fix probably belongs in crush-ast's root
`Cargo.toml` (`exclude = [".jagent"]`) or in squadron's SQ-204 sibling-link scheme, but check it before relying
on `cargo fmt` from a horse's worktree. Workaround: `rustfmt --edition 2024 --check <files>` directly.
