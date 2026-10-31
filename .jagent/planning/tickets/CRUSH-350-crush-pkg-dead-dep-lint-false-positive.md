# CRUSH-350 — crush-pkg's unused-dependency lint flags every dependency used through its functions

| Field | Value |
|-------|-------|
| **ID** | CRUSH-350 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M4 |
| **Assignee** | unassigned |
| **Dependencies** | none (found with CRUSH-348) |
| **Estimated effort** | S |

## Problem

`crush-pkg build` (and the bare flow) prints

```
note: capsule.toml:9: dead-code: dependency `dep` declared in [dependencies] but not referenced by entry file
```

for a dependency the entry file **does** use. The lint (`crates/crush-pkg/src/builder.rs`
~252, `lint_capsule_toml_with_entry`) only checks whether the dependency's **name** appears
as a token in the entry file (`scan_entry_file_references`, ~294). Under crush-pkg's
consumption model (path dependencies are concatenated into one program; there is no
`import`, CRUSH-110), a consumer calls the dependency's *functions*: `dep_hello()`,
`facet_parse()`. It never writes `dep`. Because `_` is kept inside tokens, `facet_parse`
never matches `facet` either. So the note fires for every dependency used the only way
crush-pkg supports.

## Reproduction

The CRUSH-348 fixture (checked on the v0.9.0 release binaries, 2026-10-31):
`crush-pkg build` in `/tmp/p348/consumer` prints the note, while the bare flow runs
`dep_hello()` from that dependency and prints `hello from dep`.

## Success criteria

- [ ] A dependency counts as referenced when the entry file calls any function the
      dependency defines (the builder already reads the dependency's entry to concatenate it).
- [ ] The name-token check stays only as a fallback when the dependency's source can't be read.
- [ ] Tests: dependency used through a function → no finding; declared and never called →
      finding; hyphenated dependency name.
