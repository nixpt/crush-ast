# CRUSH-118 — Prove `io.read` end-to-end: a real interactive demo

| Field | Value |
|-------|-------|
| **ID** | CRUSH-118 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
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

- [ ] A real program exercises `io.read` for actual control flow (not just a
      capability-registration smoke test)
- [ ] Verified with piped stdin input against expected output, documented in
      the commit/PR
- [ ] Added to `examples/crush/`

## Files to modify

- `examples/crush/forth.crush` or `examples/crush/brainfuck.crush` (extend),
  or a new `examples/crush/<name>.crush`

## Gates

CRUSH-115 (`io.read` must exist first).
