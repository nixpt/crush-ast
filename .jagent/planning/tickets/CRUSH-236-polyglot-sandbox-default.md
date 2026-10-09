# CRUSH-236 — Polyglot blocks run unsandboxed by default, and pinned deps are then silently ignored

| Field | Value |
|-------|-------|
| **ID** | CRUSH-236 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M4 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-20, CRUSH-66 |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 use-case review (agents + polyglot), on `main` `2d71b96` |

## Problem

`sandboxed-polyglot` is not a default feature (`crates/crush-vm/Cargo.toml:84,94`). Without
it, a granted `@python`/`@node`/`@bash` block is `Command::new(binary)` with the host's full
authority, and `@lang[pypi:…]` deps are dropped (`crates/crush-vm/src/scheduler.rs:~296-307`,
`let _ = deps;`). So `polyglot.python` reads as a narrow grant but is "run arbitrary code
as me". The live bwrap tests are cfg-gated and CI only `cargo check`s the feature, so the
sandbox path never executes in CI. buckets falls back to proot when bwrap is missing.

## Success criteria

- [ ] Unsandboxed execution requires an explicit opt-in (e.g. `--polyglot-unsandboxed`),
      or the sandbox is on by default; the default never silently runs unsandboxed.
- [ ] A block with pinned deps errors when it can't honour them.
- [ ] CI runs the sandboxed-path tests (install bwrap on the runner).
- [ ] `crush doctor` and the docs say which mode a build is in.
