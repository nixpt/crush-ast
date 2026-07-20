# CRUSH-19 — `CAP_CALL` has no wall-clock timeout (prerequisite for CRUSH-20)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-19 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none, but is a stated prerequisite for CRUSH-20 (buckets-sandboxed polyglot) |
| **Estimated effort** | M |

## Problem

`CRUSHAST-CAPTIMEOUT-1` (merged, shipped) added a `Quotas.max_wall_time_ms`
wall-clock bound specifically to `EXEC_LANG` (the polyglot-subprocess
opcode) — but its own ticket explicitly scoped that out from the generic
`CAP_CALL` dispatch path: *"Deliberately does NOT cover CAP_CALL's generic
`HostCap::call()` dispatch — `Value`'s `Rc<RefCell<...>>` isn't `Send`, so
an arbitrary trait call can't safely be preempted from another thread
without making `Value` Send first."*

`crates/crush-vm/src/scheduler.rs`'s `dispatch_cap` (called from the
`CAP_CALL` opcode handler) invokes `handler.call(args)` synchronously,
in-loop, with no bound at all — confirmed by reading the code, no timeout
wrapper anywhere in the call chain. Only step/call-depth/output quotas
exist; none of them catch a capability call that is *executing* but slow
(e.g. blocked on network I/O, a cold resource-provisioning fetch, a stuck
external process) rather than caught in an infinite instruction loop.

## Impact

Any `HostCap` implementation that can block (network fetch, filesystem
wait, and — the motivating case — a cold `buckets` resource provision for
CRUSH-20's sandboxed polyglot path) can hang the interpreter indefinitely
with no quota to stop it. This is a real, structural gap independent of
CRUSH-20 — it applies to any slow capability today (e.g. `net.http_get`
against an unresponsive server) — but CRUSH-20 specifically would make it
worse by routing `@python[deps]` through a cold-provision-capable path
that can legitimately take seconds (buckets' own measured cold-provision
latency: 347ms–4.4s depending on language, per
`SPIKE_RESULTS.md`/`SPIKE_RESULTS_2.md`).

## Reproduction

Not yet reproduced with a concrete hanging `HostCap` (would need one that
blocks indefinitely, e.g. a mock cap that sleeps forever) — this ticket is
filed from source-reading + the explicit scope-out note in
CRUSHAST-CAPTIMEOUT-1's own ticket, not from an observed hang. A
regression test should construct exactly this: a `HostCap::call()`
implementation that blocks past a configured timeout, and assert the VM
returns a named timeout error rather than hanging the test itself.

## Technical approach

`CRUSHAST-CAPTIMEOUT-1`'s own solution shape (dedicated reader threads +
poll loop against a deadline) doesn't directly transfer here, because that
was bounding a `std::process::Command` (an OS-level, externally-killable
resource) — `CAP_CALL` dispatches to an arbitrary in-process
`Box<dyn HostCap>` trait object, which has no OS handle to kill.

Options, roughly in order of how much they touch:
1. **Make `Value` (or at least the `HostCap::call()` boundary) `Send`**,
   then run the call on a dedicated thread/task with a timeout — the
   approach CRUSHAST-CAPTIMEOUT-1's own ticket flagged as the real fix,
   scoped out for exactly this reason (bigger lift, touches the `Value`
   type's `Rc<RefCell<...>>` internals broadly).
2. **Cooperative timeout**: require `HostCap` implementations that can
   legitimately block (network, provisioning) to accept a deadline
   parameter and self-enforce it internally (e.g. via a request timeout on
   the HTTP client, or a bounded poll loop matching
   `run_with_wall_clock_limit`'s shape but for whatever the cap wraps
   internally) — pushes the burden to each blocking `HostCap` impl rather
   than fixing it centrally, but avoids the `Send` refactor.
3. **Scope narrowly for CRUSH-20 first**: if a bucket-backed `HostCap`
   internally shells out to a real OS subprocess for provisioning (which
   it likely does, same shape as `EXEC_LANG`), that specific cap can reuse
   `run_with_wall_clock_limit` internally without needing the generic
   `CAP_CALL` fix — deferring the general fix while unblocking CRUSH-20.
   Worth deciding explicitly whether this narrower scope is acceptable
   before committing to the bigger (1)/(2) options.

## Files to modify

- `crates/crush-vm/src/scheduler.rs` — `dispatch_cap`, `CAP_CALL` handler
- `crates/crush-vm/src/host.rs` (or wherever `HostCap` trait is defined) —
  if option (1) or (2) is chosen, the trait signature itself may need to
  change (e.g. adding a deadline parameter to `call()`)

## Non-goals

- Preempting an already-blocked native-thread call from the *outside*
  without any cooperation from the `HostCap` implementation — genuinely
  hard in Rust without OS-level process isolation (which is what
  `EXEC_LANG`'s subprocess-based approach gets "for free"); not attempting
  true async cancellation here unless option (1) is chosen deliberately
