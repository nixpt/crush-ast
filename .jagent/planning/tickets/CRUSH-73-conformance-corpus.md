# CRUSH-73 — Conformance corpus: expectation-annotated .crush files + one black-box runner across all engines

| Field | Value |
|-------|-------|
| **ID** | CRUSH-73 |
| **Priority** | P0 (the meta-finding killer) |
| **Status** | Backlog |
| **Phase** | Correctness spine (s412) |

## Problem

The 2026-07-14 research meta-finding: crush-ast repeatedly builds both ends of
a feature, never connects the middle, and has no test that would notice — five
documented instances (CsonParseCap never registered; source spans dropped
mid-pipeline; lambdas unparseable; JIT silent nulls; GC never called). ~80
`.crush` files exist across `examples/` and `tests/` but **zero** carry
expected-output annotations; all tests are per-crate Rust unit tests asserting
internal structures. Nothing pins observable language behavior across the
execution paths. Crafting Interpreters (`// expect:`) and mal (shared runner)
both solve this shape.

## Approach

1. Annotation format: `// expect: <stdout line>` (+ `// expect-error:`,
   `// expect-exit: N`) — document in `docs/design/`.
2. Runner (xtask or test binary): for each corpus file, run through the REAL
   pipeline (parse → compile → execute), assert stdout/stderr/exit code.
3. Engines: PortableVm + FastVm first; JIT and AOT-C join via CRUSH-77's
   differential harness reusing the same corpus.
4. Seed corpus: annotate the existing examples (start with the ones M1 fixed:
   fibonacci, arrays_and_loops) + `tree-sitter-crush/test_lambda.crush` (which
   will fail until CRUSH-75 — mark xfail, that's the point).
5. CI lane running the corpus on every push.

## Definition of done

- [ ] ≥30 annotated corpus files run green through PortableVm + FastVm in CI
- [ ] Runner supports expect / expect-error / expect-exit + xfail
- [ ] At least one currently-broken behavior captured as xfail (lambda syntax)
- [ ] `docs/design/` doc for the format; corpus growth is a stated contribution norm

## Files in scope

- New: `tests/conformance/` (or `xtask` subcommand), corpus annotations in `examples/`
- NOT in scope: fixing behaviors the corpus exposes (file follow-up tickets)

## Gates

None. Feeds CRUSH-77; xfail-partners with CRUSH-75.
