# CRUSH-233 — Pause to approve: a host can intercept every capability call before it runs

| Field | Value |
|-------|-------|
| **ID** | CRUSH-233 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M5 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-232 |
| **Estimated effort** | M |
| **Filed by** | claude — 2026-10-09 use-case review (agents + polyglot), on `main` `2d71b96` |

## Problem

PortableVm pauses only on breakpoints/steps and interactive `io.read`
(`crates/crush-vm/src/portable_vm.rs:~222-231, 536-545`); every other capability call runs
synchronously inside `dispatch_cap`. A host running agent-written code therefore can
grant a capability or not, but cannot approve an individual effect (this `fs.write`, to
this path) or show the user what is about to happen. FastVM already surfaces every host
call as a `HostRequest`, but its host loop is deferred (CRUSH-163).

## Success criteria

- [ ] A host can register an approval hook (or run in a mode) where each capability call
      yields with the capability name and its arguments (as `ValueView`, redacted per
      CRUSH-160) before executing; the host resumes with approve / deny / substitute value.
- [ ] Deny is a catchable runtime error naming the capability, not `unknown capability`.
- [ ] `crush-run --approve` (interactive) and a policy-file mode (allow/deny rules by
      capability and argument pattern).
- [ ] Works the same on scheduler and PortableVm (the CRUSH-114 rule).
