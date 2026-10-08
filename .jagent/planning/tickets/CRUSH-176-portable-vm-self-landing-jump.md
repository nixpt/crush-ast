# CRUSH-176 — PortableVm skips a jump that lands on its own instruction (#94)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-176 |
| **Priority** | P1 |
| **Status** | Done (PR pending merge) |
| **Phase** | M1 (correctness spine) |
| **Assignee** | nimbus-c |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Relay lane** | C0 — prerequisite for CRUSH-159/160 (debugger steps `PortableVm`) |
| **Filed by** | foreman (derby phase 2, lane C), from GitHub #94 |

## Problem

GitHub #94: awesome-crush's `tictactoe`, `lights_out` and `blackjack` failed
on `PortableVm` (`stack underflow`, a `type error`) while the scheduler
(`crush_vm::run`) finished, and multi-round `blackjack_interactive` failed
after its last line (`stack underflow` / `truncated instruction`). crush-web's
`Session`/`execute_with`, exo-light's capsule runner and the debugger all
step `PortableVm`.

## Root cause

`PortableVm::step()` decided whether to advance the IP by comparing it before
and after the instruction ("only advance if `execute_instruction` didn't
change it"). A control-flow op that chooses the instruction it is already on
looks like "no jump":

- a recursive call in tail position: the inner `RET` returns to the caller's
  `RET`, which is the same instruction, so the caller's `RET` was skipped and
  execution fell into whatever followed (the next function, or off the end);
  `main` falling through restarted the interactive game.
- `loop: JMP loop` fell through instead of looping (noted in exo-light's
  follow-ups as "PortableVm self-jump bug").

## Fix

Control-flow ops (`CALL`, `RET`, taken `JMP`/`JZ`/`JNZ`, `THROW` to a handler,
`AWAIT`) now go through `jump_to(ip)`, which sets a `jumped` flag; `step()`
advances to the next instruction only when the flag is clear. No public API
change.

## Success criteria

- [x] the four awesome-crush programs give the same outcome and output on
      PortableVm as on the scheduler (blackjack_interactive: same as
      `crush-run` with piped stdin, four input scripts from #94)
- [x] bytecode-level regressions: recursive tail-call `RET`, self-jump
- [x] `crush-diff examples/crush/*.crush`: 0 diverged

## Evidence

- `crates/crush-lang-sdk/tests/gh_issue_94_portable_vm_parity.rs`: 4/4 failed
  before the fix (`stack underflow` ×2, `type error`, `main` re-run), 4/4 pass.
- `portable_vm::tests::test_portable_recursive_tail_call_returns_through_same_ret`
  and `test_portable_self_jump_loops_until_step_quota` fail with the old
  IP-equality check, pass with the flag (steps and stack equal the scheduler's).
- `cargo test -p crush-vm -p crush-lang-sdk`: all green.
- `crush-diff examples/crush/*.crush`: 43 files, 33 agree, 0 diverged,
  10 don't compile (pre-existing).
- Commit: `d917985`; PR crush-ast#101.
