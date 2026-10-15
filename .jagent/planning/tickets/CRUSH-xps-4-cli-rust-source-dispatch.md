# CRUSH-xps-4 — Dispatch walker languages through `crush run`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-4 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M3 / CLI |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-xps-3, CRUSH-xps-5 |
| **Estimated effort** | M |

## Problem

`crush run` accepts Crush source, while walker frontends are exposed through
separate tools. Users lack one obvious source-file runner, and adding Rust-only
dispatch would duplicate the frontend registry and strand other walkers.

## Success criteria

- [ ] `crush run file.crush` preserves current behavior.
- [ ] `crush run file.rs` uses the registry-backed SDK compile API after the
      Rust exact-or-loud gate passes.
- [ ] `.py`, `.js`, and `.ts` can be enabled through the same registry and
      language features, each gated on its own parity status.
- [ ] Walked `.py` module-level statements execute as `main`; `sys.exit(n)`
      maps to the process exit status when that behavior is enabled. `@python`
      blocks in `.crush` remain behind the existing `--polyglot` permission.
- [ ] `crush run`, `crush-lang-sdk`, `crush-aotc`, and bozo use the same
      extension-to-frontend registry.
- [ ] Unknown extensions and builds without Rust support fail with an
      actionable message.
- [ ] CLI integration tests prove frontend selection, output, exit status, and
      unsupported-syntax errors.
- [ ] Rust files never trigger implicit `rustc`, Cargo, or shell execution.
- [ ] The `.rs` slice is coordinated with CRUSH-zorro-27 / PR #201 and is not
      implemented twice.

## Technical approach

- Dispatch through the shared `WalkerAdapter` / extension registry in the
  existing runner entry point.
- Reuse SDK compilation, runtime, quotas, and capability registration.
- Keep explicit flags / host configuration as the only way to add external
      capabilities.
- Preserve `crush-run` behavior for runtime errors, stdout, and exit status;
  do not route through the current `crush-walk-run` behavior.

## Non-goals

- Executing Cargo projects or Rust binaries.
- Changing any language subset; each needs its own parity gate.
