# CRUSH-185 — Stdlib correctness sweep: `path.normalize`, `conv.to_bool`, `time.parse`, negative indices, arity, caps listing

| Field | Value |
|-------|-------|
| **ID** | CRUSH-185 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

Small wrong results found while exercising each default stdlib cap:

- `path.normalize("/../etc")` → `etc` (absolute becomes relative; `ParentDir` pops
  `RootDir`, `stdlib.rs:~937`). `path.normalize("a/../../b")` → `b` (expected `../b`).
  `system.path_normalize` always makes paths absolute (`sbl/sbl_core.crush:11`) — at least document it.
- `conv.to_bool("false")` → true.
- `time.parse("2026-01-01", "%Y-%m-%d")` → "not enough for unique date and time" (date-only formats unsupported).
- Negative indices are inconsistent: `"abc"[-1]` → `c`, but `str.char_at("abc",-1)`
  and `str.substring("abc",-2,3)` → `""`; `collections.chunk(x,-1)` returns the whole
  array while chunk size 0 errors.
- Fixed arity that the docs/caps listing don't reflect: `process.exec` takes exactly 2
  (listing says `CMD [ARGS...]`); `str.format`, `math.min`/`max`, `path.join` take exactly 2.
- `regex.match` can't be called from source (`match` is a keyword: "Expected field name but found `match`").
- `0.0/0.0` and `1.0/0.0` raise "division by zero"; `math.sqrt(-1)` returns NaN — pick one IEEE story.
- `collections.len(map)` errors "expected array or string".
- `crush-run caps` (text) is out of step with the registry: shows Graphics caps in a
  build without the feature; omits `math.random*`, `math.seed`, `str.trim_start/_end`,
  `caison.parse`/`cson.parse`, `conv.chr`/`ord`. (`caps --json` is correct — generate the
  text listing from the same catalog.)

## Success criteria

- [ ] Each bullet fixed or documented, with a unit test per fix.
- [ ] `crush-run caps` text and `--json` come from the same source.
