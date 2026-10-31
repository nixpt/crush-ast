# CRUSH-349 — `crush-pkg` treats `sys.exit(n)` as a runner failure and drops output printed before it

| Field | Value |
|-------|-------|
| **ID** | CRUSH-349 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | M4 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-348 (same entry point) |
| **Estimated effort** | S |

## Problem

A program that ends via `sys.exit(0)` — the normal shape of a CLI that prints
`--help` then exits cleanly — is reported by `crush-pkg` as a **failure**, and
everything printed before the exit is lost:

```
$ crush-pkg            # program: print("before exit"); sys.exit(0)
…
running exit-probe v0.1.0
crush-pkg[E-RUN]: program exited with status 0
$ echo $?
1
```

`sys.exit(n)` surfaces as `VmError::Exit(n)`; the runner maps any `Err` to
`E-RUN` without checking for that variant, so exit 0 is an error and exit 2
(a *correct* CLI usage error) is indistinguishable from a crash. The captured
stdout is discarded on the error path, so `--help` text never reaches the
user. `run_blob`/`run_with_caps_streaming` already guarantee "output before
`sys.exit` survives" (see CRUSH-244's notes) — the crush-pkg runner loses it.

Found 2026-10-31 testing `facet` (its `--help` / bad-argument paths both use
`sys.exit`); same program runs correctly via `crush-run run <artifact>`.

## Reproduction

```bash
mkdir -p /tmp/exit-probe && cd /tmp/exit-probe
printf '[capsule]\nname = "exit-probe"\nversion = "0.1.0"\nentry = "main.crush"\nlanguage = "crush"\ncategory = "cli"\n' > capsule.toml
printf 'fn main() {\n    print("before exit")\n    sys.exit(0)\n}\n' > main.crush
crush-pkg          # expect: "before exit", exit status 0
                   # actual: E-RUN "program exited with status 0", exit status 1, no output
```

## Success criteria

- [ ] `sys.exit(0)` exits the `crush-pkg` process with status 0 and no error
      banner.
- [ ] `sys.exit(n)` for n != 0 exits with status n, still without an `E-RUN`
      crash banner (the program chose its code).
- [ ] stdout printed before the exit is preserved (streamed or replayed).
- [ ] Genuine runtime errors keep the current `E-RUN` treatment.
- [ ] Regression tests cover exit 0 / exit 2 / panic paths through both
      `crush-pkg run` and the bare flow.

## Technical approach

- In `crates/crush-pkg/src/main.rs` (~729–745) and the runner call sites,
  match on `Err(VmError::Exit(code))` before the generic failure mapping:
  propagate the code as the process exit status.
- Preserve output: use the streaming run variant
  (`run_with_caps_streaming`, as `crush-web`'s `run_blob` does) or replay the
  captured output on the exit path.
- Check the bare-flow `run.args` plumbing at the same time — `crush-pkg --
  --help` currently routes through the same code and hid the help text.

## Files to modify

- `crates/crush-pkg/src/main.rs` — `handle_run` + bare-flow branch.
- `crates/crush-pkg/src/runners.rs` — exit-code/output propagation if needed.
- `crates/crush-pkg/tests/` — exit-path fixtures.

## Non-goals

- Changing `crush-run`'s own exit-code contract (already correct: help 0,
  usage errors 2).
- Signal handling / timeouts.

## Resolution

Done on `agent/nova/CRUSH-348-350` (2026-10-31). `CrushRunner::run_program_with_plugins_before` streams output via `run_with_caps_streaming` (so output before an exit or error survives) and maps `VmError::Exit(n)` to the new `ExecutionResult::Exit(n)`; `finish_execution` ends the process with status `n` (0 = success), no `E-RUN`. Genuine runtime errors keep `E-RUN` (exit 1) and now also keep earlier output. Tests: `test_run_exit.rs` (`sys_exit_zero_is_success_and_keeps_output`, `sys_exit_n_is_the_process_status_without_a_crash_banner`, `genuine_runtime_errors_keep_e_run_and_earlier_output`), each through `run` and bare.
