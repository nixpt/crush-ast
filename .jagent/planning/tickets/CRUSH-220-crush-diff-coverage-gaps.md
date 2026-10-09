# CRUSH-220 — `crush-diff` misses almost every cross-engine bug: FastVM abstains, JIT/AOT not included

| Field | Value |
|-------|-------|
| **ID** | CRUSH-220 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-77 |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

Every P0 engine bug from the 2026-10-09 sweep (CRUSH-187, 189, 213–216) was reported
as "agrees" by `crush-diff`, except the random map-order case:

- FastVM "abstains" on any program using `io.print` (no caps in the harness) — 30/38 examples.
- JIT and AOT aren't run at all (CRUSH-77 scope; still Backlog).
- `crush-aotc benchmark` warns "FastVM output Null diverges" for the same reason
  (FastVM tier runs without caps and reads `main`'s return).
- Bad paths / `--help` → exit 0 (see CRUSH-212).

## Success criteria

- [ ] FastVM and JIT get a real `io.print` (and the default stdlib) in the harness.
- [ ] AOT (gcc at least) included where the program is AOT-supported.
- [ ] Composite values compared via a canonical formatter (CRUSH-217) so map order doesn't create noise.
- [ ] The sweep's repro programs added to the corpus as regression cases.

## Update 2026-10-09 (AOT test drive, `main` `5755262`)

- `crates/crush-aot/tests/differential_aot.rs` compares only `main`'s return value,
  never stdout, so every print-path bug in CRUSH-216/217 passes it.
- `jit-runner` exits `host request (unserviced)` on the first `FastYield::Request`, so
  no program that calls `io.print` can run under the JIT from any CLI.

## Progress 2026-10-09 (PR #126)

AOT stdout is now covered outside `crush-diff`:

- `crates/crush-aot/tests/examples_parity.rs`: every `examples/crush` program the VM
  runs and an AOT backend accepts must print the same on the Rust and C (gcc)
  backends; known exceptions are listed with a reason, and the list fails when it
  goes stale. Against the pre-fix code generators it reports all of CRUSH-214/216.
- `crates/crush-aot/tests/stdout_parity.rs`: targeted print-path cases.

Still open here: FastVM and JIT in `crush-diff` (and `jit-runner` servicing
`io.print`), and AOT inside `crush-diff` itself.
