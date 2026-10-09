# CRUSH-224 — `use @lang` imports never reach later blocks; per-block `imports` are dropped

| Field | Value |
|-------|-------|
| **ID** | CRUSH-224 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
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

- [ ] The repro prints `sqrt16 = 4.0`.
- [ ] Selective form `use @lang python "os.path" { "join" }` makes `join` usable.
- [ ] JS works the same way (`use @lang javascript "fs" as fs`).
- [ ] Guest error line numbers still point at the right `.crush` line.
- [ ] `LangBlock.imports` is populated in `--emit ast` output.
- [ ] Both VMs (interp scheduler + PortableVm) covered by a test.

## Related

- CRUSH-193 (marshaling holes in the same pass), CRUSH-225 (state that persists
  between blocks), CRUSH-166 (guest→host callbacks).
