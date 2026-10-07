# CRUSH-118 — Prove `io.read` end-to-end: a real interactive demo

| Field | Value |
|-------|-------|
| **ID** | CRUSH-118 |
| **Priority** | P3 |
| **Status** | Done (2026-10-07) |
| **Phase** | M1 |
| **Assignee** | nimbus |
| **Dependencies** | CRUSH-115 |
| **Estimated effort** | S |

## Problem / motivation

This whole `examples/crush`/`awesome-crush` arc has run on one rule: a claim
of "done" isn't trusted until something real exercises it — compile it, run
it, check the output by hand. `io.read` (CRUSH-115) shouldn't ship without
the same treatment: a capability existing in the registry is not the same as
it working end-to-end through a real program.

## Approach

Once CRUSH-115 lands, do ONE of:

- Extend `forth.crush` or `brainfuck.crush` with a real interactive mode:
  read the program text itself from stdin via `io.read` (looping until EOF
  to assemble multi-line input) instead of the hardcoded demo strings in
  `main()`, then run it exactly as today. This turns both interpreters from
  "runs a canned demo" into "runs whatever program you actually give it" —
  the natural completion of the hosted-language-interpreter idea.
- Or write a small new example (a number-guessing game, a simple prompt-loop
  calculator) that is genuinely driven by real user input rather than a
  simulated/self-playing one — the first entry in this collection that
  isn't deterministic-by-construction.

Either way: add it to `examples/crush/` and (optionally) `awesome-crush`,
matching the established pattern, with a note on how it was verified (piped
stdin input + expected output, same as this session verified every other
program by hand rather than trusting a "done" claim).

## Definition of done

- [x] A real program exercises `io.read` for actual control flow (not just a
      capability-registration smoke test)
- [x] Verified with piped stdin input against expected output, documented in
      the commit/PR
- [x] Added to `examples/crush/`

## Outcome (2026-10-07, nimbus — crush-ast#91 §3/§4a/§4b)

The demo is awesome-crush's `games/blackjack_interactive.crush` (bet, hit or
stand, leave), copied to `examples/crush/`. Captain's ask was to make it
playable on crushlang.org/playground, where `io.read` had no stdin and
returned `""` on every read, so the game left the table after 92 steps.

- **crush-vm:** `crush_vm::InputSource` (`io_read.rs`) — `Stdin` (default,
  native behaviour unchanged), `Supplied(text)` (lines, then `""` EOF), and
  `Interactive` (host feeds lines). `PortableVm::set_input` /
  `provide_input` / `close_input` / `take_output` / `steps`. In interactive
  mode, `step()` checks *before* executing a `CAP_CALL "io.read"`: with no
  line pending it returns `VmYield::HostCall { capability: "io.read" }` and
  leaves IP, stack and the step count untouched, so after `provide_input` the
  next `step()` just runs the read. Nothing re-executes and no output is lost.
  An undeclared or denied `io.read` doesn't pause; it errors as before.
- **crush-web:** `execute_with(source, { stdin?, max_steps? })` →
  `{ ok, output, error?, steps }` (output before an error kept, #91 §3) and
  `new Session(source, { max_steps? })` with `run()` / `provide(line)` /
  `close()` / `transcript()`, each returning
  `{ status: need_input|done|error, output, error?, steps }` (`output` is what
  that call printed). `execute()` is unchanged.
- **Verified:** native `crush run` with piped `10/h/s/0` (1620 steps) is
  byte-identical to headless Chromium's `execute_with` output, and the
  Session transcript equals both (`crates/crush-web/scripts/browser-test.sh`,
  CI job `web`). Unit tests: `io_read` (supplied/interactive/EOF),
  `portable_vm` (supplied + EOF, pause/resume with IP/steps unchanged,
  undeclared doesn't pause), and crush-web `tests/io_read.rs` (7).
- **Not done:** the scheduler (`crush_vm::run`, used by `crush-run` and
  crush-web `execute()`) still reads process stdin only — gap filed in TASKS.

## Files to modify

- `examples/crush/forth.crush` or `examples/crush/brainfuck.crush` (extend),
  or a new `examples/crush/<name>.crush`

## Gates

CRUSH-115 (`io.read` must exist first).
