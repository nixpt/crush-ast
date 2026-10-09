# CRUSH-238 — Walkers turn unsupported constructs into Null instead of refusing; docs overclaim

| Field | Value |
|-------|-------|
| **ID** | CRUSH-238 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M4 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-35, CRUSH-196..204 |
| **Estimated effort** | M |
| **Filed by** | claude — 2026-10-09 use-case review (agents + polyglot), on `main` `2d71b96` |

## Problem

- `crush-walk-run` registers ternary, slice, `in` and `is` as host caps that return `Null`
  (`crates/crush-aot/src/bin/walk_run.rs:~67-79`), so a walked Python ternary prints `null`
  rather than failing. Walked code also takes Crush semantics (Python `7/2` → `3`;
  `len` counts bytes).
- Docs claim more than exists: `crush-walker-core/README.md` describes a tree-sitter-only
  walker set (Python uses rustpython-parser, JS swc, Rust syn, Bash brush-parser);
  `crates/crush-lang-sdk/src/differential.rs` header claims six engines but JIT/AOT are
  `None`; CRUSH-103 says no walker has an AOT path, but `crush-aotc file.py` works;
  `walk_run.rs:16` says "all 11 walkers" (Zig/Wasm/Dart are stubs, Java panics).

## Success criteria

- [ ] A construct a walker can't lower faithfully is a compile error naming it, never a
      Null-returning stub.
- [ ] Each walker's README/docs state supported subset and semantic differences.
- [ ] The four stale docs above corrected; CRUSH-103 updated.
