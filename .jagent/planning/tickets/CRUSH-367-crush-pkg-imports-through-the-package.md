# CRUSH-367 — crush-pkg: imports through the package (src/ + dependencies by name), standard layout

| Field | Value |
|-------|-------|
| **ID** | CRUSH-367 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | M4 |
| **Assignee** | satya |
| **Dependencies** | CRUSH-110 (source-level imports, #366), CRUSH-351 (git deps) |
| **Estimated effort** | M |

## Problem

CRUSH-110 gave Crush source-level `import`, but crush-pkg still concatenated
every dependency's entry into one program, so `import facet { facet_run }` from
a package with a facet git dependency could not resolve (facet's checkout is in
the cache, not on any search path), and a file deep in `src/` could not import a
sibling from `src/`. squeeze (nixpt/squeeze SQUEEZE-8) needs both to be the
"cargo for Crush" front door over crush-pkg.

## Success criteria

- [x] Import mode: an entry with a top-level `import` is compiled as a file with
      `src/` as a search root and each dependency (path and git) plus the package
      itself importable by its name — no concatenation.
- [x] `import NAME` → the dependency's `entry` file; `import NAME.a.b` →
      `a/b.crush` beside it; `-` in a package name becomes `_`.
- [x] Legacy concatenation kept for entries without `import` (existing tests green).
- [x] `crush-pkg new NAME [--lib|--cli]` scaffolds the standard layout
      (`src/main.crush` or `src/lib.crush`, `tests/`, `.gitignore`); `--cli`
      uses `import facet { facet_run }` with facet @ v0.2.0.
- [x] `PackageBuilder::build_file` compiles a `tests/*.crush` file with the
      package's imports (what `squeeze test` uses).
- [x] MANIFEST.md documents the layout and both modes.

## Resolution

`crush-frontend`: `ImportOptions::packages` (`ImportPackage { name, root, entry }`),
consulted before the search path (`import_system.rs` `Resolver::find`).
`crush-pkg`: `PackageBuilder::build` picks the mode with `builder::uses_imports`;
`import_options` builds the roots; `compile_file` compiles through
`crush_lang_sdk::compile::compile_crush_file_to_casm`. Tests:
`crush-frontend/tests/import_packages.rs` (5), `crush-pkg/tests/test_imports.rs` (7),
`test_new_cli.rs` extended. Not done: dependencies of dependencies are still not
followed (unchanged limitation, see MANIFEST.md).
