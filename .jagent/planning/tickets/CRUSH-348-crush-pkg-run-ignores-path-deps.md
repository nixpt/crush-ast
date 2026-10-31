# CRUSH-348 — `crush-pkg run` ignores `[[dependencies]]` path deps (recompiles the entry alone)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-348 |
| **Priority** | P2 |
| **Status** | Done |
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

Self-contained (the original case was `facet`, which isn't public). Checked on the
v0.9.0 release binaries, 2026-10-31:

```bash
mkdir -p /tmp/p348/dep /tmp/p348/consumer
printf '[capsule]\nname = "dep"\nversion = "0.1.0"\nentry = "dep.crush"\nlanguage = "crush"\ncategory = "library"\n' > /tmp/p348/dep/capsule.toml
printf 'fn dep_hello() {\n    return "hello from dep"\n}\n' > /tmp/p348/dep/dep.crush
cd /tmp/p348/consumer
printf '[capsule]\nname = "consumer"\nversion = "0.1.0"\nentry = "main.crush"\nlanguage = "crush"\ncategory = "cli"\n\n[[dependencies]]\nname = "dep"\npath = "/tmp/p348/dep"\n' > capsule.toml
printf 'fn main() {\n    print(dep_hello())\n}\n' > main.crush
crush-pkg build < /dev/null   # ok: "done: 2 function(s)"
crush-pkg < /dev/null         # bare flow: prints "hello from dep"
crush-pkg run < /dev/null     # crush-pkg[E-RUN]: Undefined function: dep_hello
```

(`build` and the bare flow also print a false `dead-code: dependency \`dep\` … not
referenced by entry file` note here — that's CRUSH-350.)

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

## Resolution

Done on `agent/nova/CRUSH-348-350` (2026-10-31). `crush-pkg run` and a bare `crush-pkg` now share one `build_and_run` path (`crates/crush-pkg/src/main.rs`): build entry + path deps, write `target/`, run the built program. Script/Native capsules, `.casm` entries and strict-mode unknown formats keep the direct-payload path (`run_payload`). `run` compile errors now carry `E-BUILDER`. Tests: `crates/crush-pkg/tests/test_run_exit.rs` (`run_sees_path_dependencies_like_build_and_bare`, `run_reports_compile_errors_with_the_builder_code`); `test_default_flow` control inverted.
