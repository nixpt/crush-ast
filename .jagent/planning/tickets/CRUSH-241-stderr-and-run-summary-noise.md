# CRUSH-241 — Scripts can't write to stderr; `crush-run` always prints `[steps=…]`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-241 |
| **Priority** | P2 |
| **Status** | In review |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | XS |
| **Filed by** | claude — 2026-10-09 scripting test drive (`tests/scripting_parity.rs`) |

## Problem

- There is no way to print to stderr (`io.eprint`), so a script's errors and diagnostics
  mix with its data on stdout, and `script | next` can't separate them.
- Every `crush-run run` prints `[steps=N, stack=M]` to stderr, which is noise for scripts
  (it's useful when debugging).
- Capability failures read `unknown capability: fs.cat: fs.cat x: No such file…` (see the
  TASKS.md issue): the prefix is wrong for a registered capability that failed.

## Success criteria

- [x] `io.eprint(x)` (ambient, `stderr/write`), streamed like `io.print`.
- [x] `[steps=…]` only with a flag (e.g. `--stats`) or `CRUSH_DEBUG`, not by default.
- [x] A failing registered capability is reported as `<cap>: <message>`.

## Resolution

- `io.eprint` (`crates/crush-vm/src/caps.rs`, registry; `scheduler.rs` `dispatch_cap`, `portable_vm.rs` `dispatch_cap`): own stderr buffer, same line shape and output quota as `io.print`. `VmResult.stderr`, `PortableVm::take_stderr`, `run_with_caps_streaming_split` / `Runtime::run_streaming_split` stream it live. AOT Rust and C backends emit `eprint!` / `fputs(.., stderr)`; `crush-aotc` (no stderr in its C runtime) bails with a named error instead of its silent-null fallback. FastVM/JIT take their capabilities from the host, so there is nothing built in to extend.
- `[steps=…]` (`crates/crush-lang-sdk/src/bin/crush-run.rs`): only with `--stats` or `CRUSH_DEBUG` (non-empty, not `0`).
- Failing registered capability: new `VmError::CapFailed { cap, message }` (`vm.rs`), used by both engines' host-cap paths; displays as `<cap>: <message>`. `UnknownCap` is left for names with no handler.
- Tests: `crates/crush-lang-sdk/tests/stderr_and_stats.rs` (3), `crates/crush-vm/src/tests/capabilities.rs` (4), `crates/crush-aot/tests/backend_agreement.rs::io_eprint_goes_to_stderr_on_both_backends`.
- Not done: stderr of `parallel_map` workers and `__repr__` hook runs is dropped (filed separately).
