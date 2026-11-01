# CRUSH-240 — Scripts can't tell files from directories, read sizes/times, or append

| Field | Value |
|-------|-------|
| **ID** | CRUSH-240 |
| **Priority** | P2 |
| **Status** | In review |
| **Phase** | M1 |
| **Assignee** | opencode (deepseek-v4.1-flash) |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 scripting test drive (`tests/scripting_parity.rs`) |

## Problem

`fs.find(".", "*")` returns directories as well as files and nothing tells them apart:
there is no `fs.is_dir`, `fs.is_file` or `fs.stat` (size, modified time). `fs.write`
always overwrites; there is no `fs.append`. `find -type f`, `du`, "newer than", and log
appending all need these. `tests/scripting_parity.rs` case `t13_count_files` pins the
first.

## Success criteria

- [x] `fs.stat(path)` → `{type, size, modified}` (or `fs.is_dir`/`fs.is_file`), inside
      `--fs-root`, with `fs/read` effects; `fs.find` gains a type filter or docs say how.
- [x] `fs.append(path, text)` with `fs/write` effects.
- [x] `t13_count_files` removed from `KNOWN_GAPS`.

## Resolution

`fs.is_dir` already existed on `main`; the rest is new.

- `fs.is_file(path)` → 1 when the path names a regular file in the sandbox, else 0
  (symlinks followed, like `fs.is_dir`).
- `fs.stat(path)` → map `{type, size, modified}`:
  - `type`: `"file"`, `"dir"`, `"symlink"` or `"other"` (socket, device, …), following
    symlinks as `stat(2)` does — a link to a file reports `"file"`.
  - `size`: bytes for a regular file, `0` for anything else. A directory's platform
    size (4096 on Linux) is not portable across the disk and in-memory backends, so it
    is normalised to 0.
  - `modified`: seconds since the Unix epoch (UTC), the same unit as `time.now`.
  - A missing path is an error (`fs.stat nope: No such file or directory (os error 2)`),
    naming only the sandbox path.
- `fs.append(path, data)` → creates the file if missing, appends otherwise, with the same
  payload rules as `fs.write` (string, `bytes`, buffer, other data as its text form; a
  function/handle is refused). Effects `fs/write`; added to crush-vm's
  `PRIVILEGED_PREFIXES` next to `fs.write`.
- `fs.find(dir, pattern, type)` — optional third argument, the `find -type` filter:
  `"file"`, `"dir"`, `"symlink"` or `"other"`. It matches the entry's own type (a symlink
  is `"symlink"`, not what it points at) and still recurses into directories. An unknown
  type is an error, not an empty list.

Files:

- `crates/crush-lang-sdk/src/fs_backend.rs:41` — `Metadata {kind, size, modified}` and
  the `FsBackend::{metadata, append}` primitives; `DiskFs` uses `std::fs::metadata` and
  seeks to the end of `open_for_write(path, false)`; `MemFs` (crush-web) now keeps a
  per-node modification time (`fs_backend.rs:207`), moved by writes, appends, touches
  and by adding/removing/renaming entries in a directory, so `fs.stat` answers there too.
- `crates/crush-lang-sdk/src/host_caps.rs:759` `FsAppendCap`, `:902` `FsIsFileCap`,
  `:947` `FsStatCap` (registration at `:853`), all on the shared `FsSandbox`, so every
  engine (PortableVm, FastVM, JIT, AOT) gets them through the one capability registry.
- `crates/crush-lang-sdk/src/fs_tools.rs:286` `find_type` and the type filter in `find`.
- `crates/crush-lang-sdk/src/effects.rs:34` — `fs.stat`/`fs.is_file` are `fs/read`,
  `fs.append` is `fs/write`; `crates/crush-vm/src/caps.rs:288` — `fs.append` privileged.
- `crates/crush-lang-sdk/src/bin/crush-run.rs` — `crush-run caps` lists the new names.
- `tests/scripting/cases/t13_count_files.crush` now counts `fs.find(".", "*", "file")` and
  cross-checks every result with `fs.is_file`; new `t14_append_log` appends to
  `logs/app.log` and compares with bash's `>>`.

Tests:

- `cargo test -p crush-lang-sdk --lib` — 326 passed (adds
  `fs_tools::tests::file_type_size_time_and_append`, find-filter assertions and
  `host_caps::tests::fs_append_extends_and_stays_in_the_sandbox`).
- `cargo test -p crush-lang-sdk --test fs_memory_parity` — 2 passed (12 new disk/memory
  steps; `fs.stat`'s `modified` is normalised, it is wall-clock).
- `cargo test -p crush-lang-sdk --test scripting_parity` — 1 passed (t13 and t14 match
  bash; `t13_count_files` gone from `KNOWN_GAPS`).
- `cargo test -p crush-lang-sdk --test crush_run_test` — 21 passed.
- `cargo test -p crush-vm` — 331 passed + 4 smaller suites.

Live: `crush-run run --fs --fs-root .` on a scratch program printed
`type=file size=12`, `modified is a number: true`, `is_file=1 is_dir=0`,
`dir: dir size=0`, `files: [hello.txt, stat.crush]`, `dirs: [sub]`.

Not changed: `crush-walker-core`'s `>>` lowering still reads then rewrites the file; it
could use `fs.append` in a follow-up.
