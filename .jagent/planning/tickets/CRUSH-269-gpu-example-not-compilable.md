# CRUSH-269 — GPU/PTX example `examples/gpu/q6k_gemv.crush` not compilable by the frontend

| Field | Value |
|-------|-------|
| **ID** | CRUSH-269 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M6 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |

## Problem

`examples/gpu/q6k_gemv.crush` (a Q6_K dequant → GEMV kernel for the crush→PTX backend) does not compile. It uses annotations (`@kernel`, `@target`, `@gpu`, `@semantic_switch`, `@synthesize`) and syntax (`mut`, `&`, `=>` in annotations) that the frontend doesn't recognize, e.g. `Unknown annotation: @kernel`. The file's own header says "NOT yet compilable."

It is covered by `// expect-error: Unknown annotation: @kernel` in `crates/crush-lang-sdk/tests/examples_compile.rs`, which now fails the build loudly instead of being silently skipped (the CRUSH-226/#226 gap).

## Semantics to pin (before implementing)

- The GPU annotations are lowered by the crush→PTX pipeline, not the base frontend; decide whether `q6k_gemv.crush` belongs in the frontend's compile surface at all, or whether the expect-error is permanent because it targets a separate backend.
- If the example is meant to compile under `crushc`/`crush-run`, implement the `@kernel`/`@target`/`@gpu` annotations in `crush-frontend` (compiler source — out of scope for the test-only fix in this task).

## Success criteria

- [ ] `examples/gpu/q6k_gemv.crush` fails to compile with an error containing `Unknown annotation: @kernel` (covered by the expect-error test).
- [ ] If GPU annotation support is added, the example compiles and the expect-error marker is removed.

## Non-goals

- Implementing the crush→PTX backend or the `@kernel`/`@target`/`@gpu` annotations.
