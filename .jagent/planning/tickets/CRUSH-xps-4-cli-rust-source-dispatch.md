# CRUSH-xps-4 — Dispatch `.rs` source through `crush run`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-4 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M2 / CLI |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-xps-3 |
| **Estimated effort** | M |

## Problem

`crush run` accepts Crush source, while the Rust frontend is exposed through a
walker-oriented path. Users lack one obvious file-run command for the two
supported source languages.

## Success criteria

- [ ] `crush run file.crush` preserves current behavior.
- [ ] `crush run file.rs` uses the Rust-source SDK API and runs in the ordinary
      Crush runtime.
- [ ] Unknown extensions and builds without Rust support fail with an
      actionable message.
- [ ] CLI integration tests prove frontend selection, output, exit status, and
      unsupported-syntax errors.
- [ ] Rust files never trigger implicit `rustc`, Cargo, or shell execution.

## Technical approach

- Dispatch on the source path extension in the existing runner entry point.
- Reuse SDK compilation, runtime, quotas, and capability registration.
- Keep explicit flags / host configuration as the only way to add external
      capabilities.

## Non-goals

- Executing Cargo projects or Rust binaries.
- Changing the Rust language subset.
