# CRUSH-240 — Scripts can't tell files from directories, read sizes/times, or append

| Field | Value |
|-------|-------|
| **ID** | CRUSH-240 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
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

- [ ] `fs.stat(path)` → `{type, size, modified}` (or `fs.is_dir`/`fs.is_file`), inside
      `--fs-root`, with `fs/read` effects; `fs.find` gains a type filter or docs say how.
- [ ] `fs.append(path, text)` with `fs/write` effects.
- [ ] `t13_count_files` removed from `KNOWN_GAPS`.
