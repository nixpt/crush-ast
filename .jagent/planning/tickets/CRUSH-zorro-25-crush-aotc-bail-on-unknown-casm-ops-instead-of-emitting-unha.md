# CRUSH-zorro-25 — crush-aotc: bail on unknown CASM ops instead of emitting /* unhandled op */

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-25 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M5 (backends) |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude (vega) — 2026-10-15, from the foreman's review of #194 |

## Problem

`crush-aotc` (`crates/crush-aotc/src/codegen.rs`) silently drops what it can't compile, so its
output can be wrong with no error — against the exact-or-loud rule the other AOT backends follow
(`crush-aot`'s `UnsupportedOps`):

- `_ => writeln!(out, "    /* unhandled op: {} */", instr.op)` — any CASM op it doesn't know
  becomes a C comment and the program continues without it (~line 460).
- `writeln!(out, "    /* unimplemented cap: {} */", cap_name)` — same for capabilities (~line 438).

#194 special-cased only `make_closure` / `call_value` (CRUSH-251) to bail; everything else still
drops silently.

## Fix

Collect unknown ops and caps and fail `AotcCompiler::compile` with the list (like
`crush_aot::UnsupportedOps`). Ops that are genuinely no-ops in C (e.g. debug/annotation ops) get an
explicit arm instead of falling into the catch-all.

## Done when

A program using an op or cap crush-aotc doesn't implement fails to compile, naming it (test), and the
existing crush-aotc suites still pass (any program they relied on being silently truncated is either
implemented or now expected to fail).
