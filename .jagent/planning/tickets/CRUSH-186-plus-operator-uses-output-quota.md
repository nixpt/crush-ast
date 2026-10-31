# CRUSH-186 — String `+` is limited by the *output* quota; wall-clock limit doesn't bound `process.exec`/loops

| Field | Value |
|-------|-------|
| **ID** | CRUSH-186 |
| **Priority** | P2 |
| **Status** | In review |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | XS |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

- Building any string over 1 MB with `+` fails with `output quota exceeded` even if
  nothing is printed (`crates/crush-vm/src/scheduler.rs:~648`), while `str.concat` is
  unlimited. The quota is the wrong one; a memory quota (CRUSH-181) should cover both.
- `max_wall_time_ms` (30 s) is not enforced across a blocking `process.exec("sleep","40")`
  or a plain `while true` with a large `--max-steps` (ran >60 s). Related: CRUSH-19/M7.

## Success criteria

- [x] `+` and `str.*` behave the same; output quota applies only to output. (Neither has a size limit now; the shared memory quota is CRUSH-235, still open.)
- [x] Wall-clock limit is checked in the step loop and applied as a timeout to blocking caps.

## Resolution

- `+` (scheduler.rs, the string `ADD` arm): removed the `max_output` check. PortableVm's `ADD` never had one, so both engines now agree with `str.concat`: no size limit. No memory quota exists on main (CRUSH-181 covered stdlib panics/allocs; CRUSH-235 is the open memory-quota ticket), so none was invented here. Gap: `s = s + s` in a loop can still exhaust host memory until CRUSH-235 lands.
- New `crates/crush-vm/src/wall.rs`: one deadline per run (thread-local, nested runs keep the earlier one). Scheduler (`run_from`) and `PortableVm::step` check it every 1024 instructions and fail with the new `VmError::WallTimeQuota` (`wall-clock quota exceeded (Nms)`). Every place that handed `max_wall_time_ms` whole to a blocking op (host caps, polyglot blocks, AI opcodes) now passes the time *remaining*, so chained calls share one budget.
- `process.exec` (`crush-lang-sdk/src/host_caps.rs`, `exec_with_wall_limit`): runs the child with piped output and a poll loop; on expiry kills it and returns `CapTimeout` naming `process.exec` and the limit. Outside a VM run (no deadline) it is the old blocking `output()`.
- Engines: scheduler and PortableVm enforce both. FastVM, JIT and the AOT backends take no `Quotas` at all (they enforce neither `max_steps` nor wall time today), so they are NOT covered; a hot `while true` there still runs until killed. Not made to fail loudly in this change (no quota plumbing to hang an error on); needs its own ticket.
- Tests (`crush-lang-sdk` `host_caps::tests`): `wall_limit_stops_while_true`, `wall_limit_kills_blocking_process_exec`, `wall_budget_is_shared_not_per_call`, `plus_builds_strings_past_the_output_quota`; the first three run on both engines with a 200 ms limit.
- Before/after: `while true` with `--max-steps 100000000000` ran >60 s; now stops at 30.0 s. `process.exec("sleep","40")` ran 40 s; now killed at 30 s.
