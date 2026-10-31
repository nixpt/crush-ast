# CRUSH-348 — `crush-pkg run` ignores `[[dependencies]]` path deps (recompiles the entry alone)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-348 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M4 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

`crush-pkg build` (and the bare `crush-pkg -- ARGS` flow) correctly resolves
path dependencies and concatenates them before compiling. But
`crush-pkg run` does not: `handle_run` in `crates/crush-pkg/src/main.rs`
~line 694 takes `payload = root/capsule.entry` and hands the **source file**
to the runner, recompiling it alone. Any program that calls a dependency's
functions then fails at run time:

```
$ crush-pkg run
running facet-consumer v0.1.0 (crush)
crush-pkg[E-RUN]: Undefined function: facet_parse
```

Found 2026-10-31 while wiring `facet` (a Crush CLI library) as a path
dependency; `build` works, `run` cannot see the dep.

## Reproduction

```bash
# /tmp/consumer/capsule.toml:  [[dependencies]] name="facet" path="/workspace/projects/facet"
# /tmp/consumer/main.crush:    fn main() { let a = facet_parse(spec); … }
cd /tmp/consumer
crush-pkg build        # ok: "resolved dep facet -> …/facet.crush", compiles entry+deps
crush-pkg run          # E-RUN: Undefined function: facet_parse
crush-pkg -- --help    # bare flow: runs (see CRUSH-349 for its exit handling)
```

## Success criteria

- [ ] `crush-pkg run` executes the same combined program `build` produces —
      either by running `target/<name>.cvm` when fresh, or by resolving deps
      before compiling.
- [ ] A regression test packages a dep + consumer fixture and asserts a
      dep function is callable via `run`.
- [ ] `run` and the bare flow agree on behavior (one entry point for
      "compile entry+deps, then execute").

## Technical approach

- `handle_run` should mirror `PackageBuilder::build`'s output path
  (`crates/crush-pkg/src/main.rs` ~694–745): build to target/ then
  `run_program`/`run_blob` on the artifact, or accept the `BuildOutput`
  from a builder call.
- Alternatively route `run` through the bare-flow code path if that already
  builds combined (verify — see CRUSH-349, which shows the bare flow runs the
  entry's compiled form).

## Files to modify

- `crates/crush-pkg/src/main.rs` — `handle_run`.
- `crates/crush-pkg/tests/` — dep-visible-to-run fixture.

## Non-goals

- Changing dependency resolution/concatenation for `build`.
- Native/script capsule runners (they skip compilation by design).
