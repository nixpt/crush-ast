# CRUSH-180 — fs sandbox escape: writes follow a dangling symlink out of `--fs-root`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-180 |
| **Priority** | P0 |
| **Status** | Backlog |
| **Phase** | M7 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

`confine()` resolves paths with `canonicalize()`. On a dangling symlink that
fails, so the path is treated as a new file under its (in-root) parent, and
the subsequent write follows the link to its target outside the root.

## Reproduction

```
mkdir box outside
ln -s ../outside/new.txt box/dangle
echo 'fs.write("dangle", "PWNED")' > p.crush
crush-run run p.crush --fs --fs-root box
cat outside/new.txt   # PWNED
```

`fs.touch("dangle")` and `fs.cp("in.txt", "dangle")` also create the outside file.
(The `fs.write` call then errors with `stack underflow` — that is CRUSH-183, separate.)

Everything else held in the same sweep: `..`, absolute paths, symlinks to existing
outside files/dirs, `fs.cd` escapes, recursive `fs.cp`/`fs.find`/`text.grep`.

## Where

`crates/crush-lang-sdk/src/host_caps.rs:353-380` (`confine`).

## Success criteria

- [ ] Writes/creates never follow a symlink whose target resolves outside the root
      (check each component with `symlink_metadata`/`read_link`, or open with
      `O_NOFOLLOW` / `openat2(RESOLVE_BENEATH)` on Linux).
- [ ] Regression tests for `fs.write`, `fs.touch`, `fs.cp`, `fs.mv`, `fs.mkdir`
      through a dangling link.
