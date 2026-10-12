# CRUSH-254 — host-driven job queue: microtasks, host callbacks, resumable runs

| Field | Value |
|-------|-------|
| **ID** | CRUSH-254 |
| **Priority** | P1 — Promises / events / timers for browser scripts |
| **Status** | Backlog |
| **Phase** | M6 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-251 (closures, for callbacks) |
| **Estimated effort** | M |

## Problem

A browser owns the event loop: it runs a script, then later calls back into it (DOM events, timers,
network completions, Promise reactions). crush-vm runs a program to completion (`run_with_caps`);
`SPAWN` exists (`bytecode.rs:213`) and crush-web has a pause/resume `Session` for `io.read`
(CRUSH-118), but there is no API for "keep this VM's heap alive, call closure #N with args, drain
microtasks, return". surfer already implements a bounded event loop around Boa (timers, network
macrotasks, `run_jobs`); the crush path needs the equivalent primitive.

## Success criteria

- [ ] A persistent `VmSession` (CVM1 + FastVM): `eval(program)`, `call(fn_handle, args)`,
      `enqueue_microtask`, `run_microtasks(budget)` → leaves heap/globals alive between calls.
- [ ] Promise-reaction jobs and `await` suspension compile to this (frontend-side in bozo; VM provides
      the queue + resumable frames/generators).
- [ ] Budgeted execution (instruction or time budget) so a host can bound a runaway script.
- [ ] Tests: callback after eval; microtask ordering (A→B→C as in JS); budget exhaustion returns control.

## Cross-refs

bozo `BOZO-M7-3`; surfer-browser `SURF-JSENGINE-1`; CRUSH-118 (Session precedent).
