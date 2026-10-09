# CRUSH-232 — Agent-written programs declare their capabilities; the host checks them before running

| Field | Value |
|-------|-------|
| **ID** | CRUSH-232 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M5 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-170 (capabilities_used) |
| **Estimated effort** | M |
| **Filed by** | claude — 2026-10-09 use-case review (agents + polyglot), on `main` `2d71b96` |

## Problem

For `.crush` source the compiler adds every capability the program uses to
`manifest.permissions` by itself (`crates/crush-frontend/src/compiler.rs:~1800-1850`),
and `crush-run --cap` is only consulted for `.casm` input
(`crates/crush-lang-sdk/src/bin/crush-run.rs:~445`). So the "manifest" says nothing a
reader didn't already have: whatever the code calls is declared. The only gate is the
host flags (`--fs`, `--env`, …), and a missing grant surfaces mid-run as
`unknown capability: …`, after the program has already done its earlier effects and
with its earlier output discarded.

For code an agent writes and a person (or policy) runs unreviewed, the useful contract
is the reverse: the program states what it needs, the host checks that against the code
and against what it is willing to grant, and refuses **before** anything runs.

## Success criteria

- [ ] A program can declare the capabilities it needs in source; the compiler rejects a
      program that uses a capability it did not declare (and, with no declaration, keeps
      today's behaviour so existing programs still compile).
- [ ] `crush-run` prints what a program needs (declared vs. inferred by
      `capabilities_used`) without running it.
- [ ] Before executing, `crush-run` refuses with one error listing every needed
      capability the host has not granted — no partial run.
- [ ] An example agent-written program with a declaration, run with and without grants.
