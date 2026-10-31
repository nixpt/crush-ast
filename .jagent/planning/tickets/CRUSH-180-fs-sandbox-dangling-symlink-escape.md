# CRUSH-180 — fs sandbox escape: writes follow a dangling symlink out of `--fs-root`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-180 |
| **Priority** | P0 |
| **Status** | Done — PR #172 merged 2026-10-14 |
| **Phase** | M7 |
| **Assignee** | gorkhali |
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

- [x] Writes/creates never follow a symlink whose target resolves outside the root
      (check each component with `symlink_metadata`/`read_link`, or open with
      `O_NOFOLLOW` / `openat2(RESOLVE_BENEATH)` on Linux).
- [x] Regression tests for `fs.write`, `fs.touch`, `fs.cp`, `fs.mv`, `fs.mkdir`
      through a dangling link.

## Result

Branch `agent/gorkhali/CRUSH-180`.

### Approach

`confine()` (`crates/crush-lang-sdk/src/host_caps.rs`) no longer uses
`canonicalize()`. It resolves the path one component at a time from the
canonical root, using `symlink_metadata` and `read_link`. Every symlink is
followed whether or not its target exists, up to 40 links (the kernel's ELOOP
limit; a loop is reported as `too many levels of symbolic links`). The resolved
prefix never contains a symlink, so `..` after it is exact. The fully resolved
path is then checked against the root. A link to a missing target outside the
root now resolves to that outside path and is refused, at any component.

Final opens for writing go through `open_for_write()`, which `fs.write`,
`fs.touch` and `fs.cp` (single file and recursive) all use:
- A path that doesn't exist is created with `create_new` (`O_EXCL`). This never
  follows a link, so a link planted after the check makes the call fail.
- An existing file is opened without truncation. Its `fstat` is compared with
  the path's `lstat` (dev+ino, and the path is not a link). Only then is it
  truncated or written.

`fs.mkdir` and `fs.mv` were not exploitable at the last component, because
`mkdir(2)` and `rename(2)` don't follow it. They now also refuse an outside link
as a middle component, and `fs.mkdir` refuses one as the last component.

Same class, also fixed: `fs.cp src dst true` into an existing `dst` only
confined `dst` itself. Links already below it were written through, e.g.
`dst/a.txt -> ../../outside/pwn` created `outside/pwn`. `copy_tree` now refuses
a link at any destination path, and its files are opened with
`open_for_write`.

Why not `openat2(RESOLVE_BENEATH)`: it needs a new `libc`/`rustix` dependency,
and it is Linux-only, so a portable fallback (this resolver) is needed anyway.
Also, no Crush capability can create a symlink, so nothing confined by the
sandbox can race it.

### Residual window

Between resolve and open, a **concurrent process outside the VM** that can
write inside the root could replace a *middle* directory component with a
symlink. That redirects the open. The last component is covered by
`O_EXCL`/`fstat` compare. Closing the middle-component window needs an fd-based
walk (`openat2`/`O_NOFOLLOW` per component). This is accepted because the only
things that can race it are external processes, `process.exec` and polyglot
blocks, and all of those already run outside the fs sandbox.

Symlinks that resolve **inside** the root keep working, dangling ones included:
writing through them creates their in-root target.

### Tests

`crates/crush-lang-sdk/src/fs_tools.rs`:
- `dangling_symlinks_never_lead_out_of_the_root`: `fs.write`/`fs.touch`/
  `fs.cp`/`fs.mkdir`/`fs.mkdir -p` through relative, absolute and chained
  dangling links; `fs.write`/`fs.touch`/`fs.cp`/`fs.mv`/`fs.mkdir -p` through a
  dangling or outside-pointing directory link as a middle component
  (`odir -> ../outside`, write `odir/x`); `fs.cp -r` onto a dangling directory
  link; a symlink loop. Asserts `outside/` is unchanged.
- `recursive_cp_never_writes_through_a_link_in_the_destination`: RED before
  the `copy_tree` change (`d1` created `outside/pwn`).
- `symlinks_inside_the_root_still_work`: an existing link, a directory link,
  a dangling in-root link, and a link that leaves and re-enters the root.

`crates/crush-lang-sdk/src/host_caps.rs`:
- `open_for_write_never_follows_a_link`.

The existing sandbox tests pass unchanged:
`every_path_argument_is_confined_to_the_root`,
`rm_and_mv_act_on_a_symlink_not_its_target`,
`cp_refuses_symlinks_inside_a_recursive_copy`,
`sandbox_rejects_parent_escapes_for_paths_that_do_not_exist_yet`,
`sandbox_follows_symlinks_for_new_files`, and the `text_tools` tests.

CRUSH-183 (`stack underflow` after `fs.write`) did not reproduce on this base.

## Survey 2026-10-31 (status)

PR #172 (`agent/gorkhali/CRUSH-180`) merged 2026-10-14. Previous status: `Done (pending merge)`.
