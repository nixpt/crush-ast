# CRUSH-224 — `use @lang` imports never reach later blocks; per-block `imports` are dropped

| Field | Value |
|-------|-------|
| **ID** | CRUSH-224 |
| **Priority** | P1 |
| **Status** | Done |
| **Phase** | M1 |
| **Assignee** | panini |
| **Dependencies** | none (touches the same `prepare_stmts` pass as CRUSH-193) |
| **Estimated effort** | S |
| **Filed by** | kai (foreman) — s476, 2026-10-09; reproduced on a 2026-10-07 debug build, code path unchanged at `5755262` |

## Problem

`use @lang python "math" as m` parses and compiles, but the module is not
available in any later `@python` block:

```crush
use @lang python "math" as m
@python {
print("sqrt16 =", math.sqrt(16))
}
```

```
crushc t.crush -o t.cvm1 && crush-run run --polyglot --stdlib t.cvm1
# NameError: name 'math' is not defined. Did you forget to import 'math'?
```

Three causes, all in `crush-frontend`:

1. `ImportStatement::PolyglotModule` lowers to a standalone
   `exec_lang "import <module>"` (`compiler.rs:947`). Under the subprocess model
   (C-2/C-8) that runs in its own throwaway interpreter, so the import does nothing,
   and the `store` of its result binds the alias to whatever the block returned.
2. `Statement::LangBlock.imports` exists in CAST but the compiler ignores it
   (`compiler.rs:806`, `imports: _, // Ignore for now`).
3. `crates/crush-frontend/src/polyglot_imports.rs` (~690 lines: import resolvers,
   cache, `PolyglotModule` synthesis) is exported from `lib.rs:11` and has no callers.

For comparison, exosphere's in-tree Crush (`crates/core/crush-lang`) has the same
three gaps, and its lexer reserves `lang`, so `use @lang …` never parsed there at all.

## Fix direction

Do it at compile time, with no state carried between processes:

- In the polyglot pass (`crush-lang-sdk/src/compile.rs`, `prepare_stmts`), collect
  every `use @lang <lang> "<module>" [as alias] [{ "a", "b" }]` in scope. Prepend the
  language's import line to the body of each later block of that language.
  - Python: `import m as alias` / `from m import a, b`.
  - JS: `const alias = require("m")` / `import`.
  - Bash: `source`, if supported, else reject.
- Fill `LangBlock.imports` from the same list, so CAST consumers see it.
- Stop lowering `PolyglotModule` to a bare `exec_lang` (or keep it only as an
  existence probe that is clearly named and returns nothing).
- Prepending lines shifts guest line numbers; adjust the `(at .crush line N)` mapping
  used in error reports.
- Either wire `polyglot_imports.rs` in, or delete it.

## Success criteria

- [x] The repro prints `sqrt16 = 4.0`.
- [x] Selective form `use @lang python "os.path" { "join" }` makes `join` usable.
- [x] JS works the same way (`use @lang javascript "fs" as fs`).
- [x] Guest error line numbers still point at the right `.crush` line.
- [x] `LangBlock.imports` is populated in `--emit ast` output.
- [x] Both VMs (interp scheduler + PortableVm) covered by a test.

## Resolution (panini, 2026-10-09)

- `prepare_polyglot_blocks` (crush-lang-sdk) fills `LangBlock.imports`: a
  `use @lang` reaches later blocks of its (canonical) language in the same
  body and nested bodies; one at the top level of `main` (where a script's
  top-level statements land) also reaches every other function.
- The compiler turns `imports` into source (`crush_frontend::lang_imports`)
  and splices it onto guest line 1: a blank first line is replaced, otherwise
  the header is prefixed (`import m; <line 1>`). Guest line K stays `.crush`
  line `block_line + K - 1`. The only shift: a Python block whose first line
  opens a block (`for …:`) gets the header on its own line.
- What a `use` binds: the module under its own name (unless selective-only),
  the alias in addition, the selected names. So the repro's `math.sqrt` works
  with `as m`, and so does `m.sqrt`. Python paths and all names are validated
  as identifiers before they are spliced into source.
- `PolyglotModule` lowers to nothing (bash and unknown languages are compile
  errors).
- `polyglot_imports.rs` **deleted**: it modelled a simulated sandbox
  (sandbox ids, memory/CPU limits nothing enforced, a hard-coded module
  allowlist) that has nothing to do with the subprocess + capability-gate
  model, and nothing in the workspace or its dependents called it.

## Related

- CRUSH-193 (marshaling holes in the same pass), CRUSH-225 (state that persists
  between blocks), CRUSH-166 (guest→host callbacks).
