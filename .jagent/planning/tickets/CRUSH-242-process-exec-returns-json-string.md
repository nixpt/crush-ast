# CRUSH-242 — `process.exec` returns a JSON string instead of a value

| Field | Value |
|-------|-------|
| **ID** | CRUSH-242 |
| **Priority** | P2 |
| **Status** | In review |
| **Phase** | M1 |
| **Assignee** | opencode |
| **Dependencies** | CRUSH-217 |
| **Estimated effort** | XS |
| **Filed by** | claude — 2026-10-09 scripting test drive (`tests/scripting_parity.rs`) |

## Problem

`process.exec(cmd, args)` returns the string `{"exit_code":0,"stderr":"","stdout":"…"}`, so
every call needs `json.parse(...)` before reading `exit_code`. And on the parsed map,
`r["exit_code"]` errors (CRUSH-217); only `r.exit_code` works. A missing command is an
uncatchable runtime error rather than a result with a failure code.

## Success criteria

- [x] `process.exec` returns a map (`exit_code`, `stdout`, `stderr`).
- [x] A command that can't be started is reported in the result (or a catchable error,
      once CRUSH-219 lands), not a run-ending error. — **result, not catchable error**;
      see Resolution for why.

## Resolution (2026-10-31, opencode, branch `agent/opencode/CRUSH-242`)

`ProcessExecCap::call` (`crates/crush-lang-sdk/src/host_caps.rs` ~1187) now returns
`Value::new_map([exit_code, stdout, stderr])` (insertion-ordered, so printing is
deterministic: `{exit_code: 0, stdout: …, stderr: …}`) instead of
`Value::Str(serde_json::json!({...}).to_string())`. The success path is unchanged
otherwise; `json.stringify(r)` still yields the same JSON the string used to carry.

**Missing command → result with a failure code, not a catchable error.** Chosen because
cap failures are not catchable yet (CRUSH-219 is still Backlog; `t11_missing_file` in
`scripting_parity.rs`'s KNOWN_GAPS pins that), so returning `Err` would keep the exact
run-ending behaviour the ticket says to remove. Spawn failure now yields
`{exit_code: -1, stdout: "", stderr: "process.exec <cmd>: <os error>"}`.
Why `-1` and not a shell-style 127/126: `process.exec` is argv-exec, not a shell — 127/126
are shell conventions that would misclassify spawn errors (NotFound vs PermissionDenied vs
resource limits), while `-1` already means "no exit status exists" in this cap (the
signal-killed `status.code().unwrap_or(-1)` path), so no process exit code (0–255) can
collide with it. The OS reason text rides on `stderr` either way. When CRUSH-219 lands,
this can move to a catchable error without touching the success path (noted in the code).

Out of scope, still open: `r["exit_code"]` bracket access on maps errors on every
engine — CRUSH-217, untouched here; dot access (`r.exit_code`) is the workaround.

Behaviour change for existing scripts (also in CHANGELOG [Unreleased]): the
`json.parse(process.exec(...))` wrapper must go. Updated in-tree consumers:
`t05_exec_status.crush` (drops `json.parse`), `tests/integration.rs`
`host_process_capabilities` (asserts `exit_code: 0` map display), `crush-run caps` help
line. New parity pair `t14_missing_command.{sh,crush}` pins the missing-command
behaviour against bash.

Tests (all green, `cargo test -p crush-lang-sdk`: 779 passed / 0 failed):

- `host_caps::tests::process_exec_returns_result_map` — map shape, key order, field types.
- `host_caps::tests::process_exec_missing_command_is_a_result_not_an_error` — `Ok` map,
  `exit_code == -1`, reason names the command on `stderr`.
- `integration.rs::host_process_capabilities` — through the VM via `CAP_CALL`.
- `scripting_parity.rs` — 14/14 cases match bash (t05 updated, t14 new).
- Live-verified with `crush-run run --process`: field reads, whole-map print, and a
  missing command no longer ends the run (program continues, exit 0).
