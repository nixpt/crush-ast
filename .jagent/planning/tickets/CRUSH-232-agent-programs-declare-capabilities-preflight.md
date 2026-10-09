# CRUSH-232 — Agent-written programs declare their capabilities; the host checks them before running

| Field | Value |
|-------|-------|
| **ID** | CRUSH-232 |
| **Priority** | P1 |
| **Status** | Done (2026-10-09) |
| **Phase** | M5 |
| **Assignee** | claude |
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

- [x] A program can declare the capabilities it needs in source; the compiler rejects a
      program that uses a capability it did not declare (and, with no declaration, keeps
      today's behaviour so existing programs still compile).
- [x] `crush-run` prints what a program needs (declared vs. inferred by
      `capabilities_used`) without running it.
- [x] Before executing, `crush-run` refuses with one error listing every needed
      capability the host has not granted — no partial run.
- [x] An example agent-written program with a declaration, run with and without grants.

## Resolution

- Syntax: `@capabilities [..]` (top level) or `capabilities: [..]` in `@module`; stored as
  `crush_cast::manifest::ModuleManifest::capabilities: Option<Vec<String>>` (`None` = no
  declaration, `Some([])` = ambient only). Entry rule = `crush-pkg check`'s `covers()`, now
  shared from `crush_lang_sdk::effects`.
- Compile check in `crush_lang_sdk::compile::compile_crush_to_casm` (every Crush compile path,
  `crushc` included), on the bytecode via `capabilities_used`, so names are the gate names
  (`polyglot.python`, lowered builtins). Ambient = `CapInfo::is_ambient`.
- Pre-run check: `Runtime::missing_grants` (built-ins, registered host caps,
  `Quotas::allowed_caps`); `crush-run run` refuses with `[capabilities] … nothing was run` and
  the grants grouped by flag; `crush-run caps FILE [--json]` shows the plan.
- Live: every example that ran before runs the same; the six that failed on a missing grant
  now list all missing grants up front (fs_test, phase2_3_test, repl_test, text_tools_test,
  lang_test, polyglot_braces).
- Example: awesome-crush `agents/log-triage/` (tool + two tampered variants + walkthrough).
- Tests: `crush_run_test.rs` (4 new), `annotation_parse_tests.rs` (3), `effects.rs` (2).
- Not done here: approving individual calls (CRUSH-233); `--cap` for `.crush` remains unused.
