# Import system

**Status:** implemented, CRUSH-110 (2026-10-31). Replaces the CRUSH-173 design note, which described
imports as parsed but never resolved. The implementation is `crates/crush-frontend/src/import_system.rs`;
its module docs are the normative rule list, this note is the overview.

## Summary

Crush module imports are resolved **at compile time**. Loading a module means parsing another `.crush`
file and merging its definitions into the importing `Program` before `SemanticAnalyzer::check`, so
imported names type-check like local ones, the capability check sees imported code exactly as if it were
local, and no runtime capability is involved. `module.load` is no longer emitted.

```crush
import facet                              // facet.facet_parse(spec)
import facet { facet_parse, facet_help }  // facet_parse(spec)
import util.strings as s                  // util/strings.crush → s.title(x)
```

```text
$ crush-run run examples/crush/imports/main.crush
== imports ==
3x4 = 12
25
```

## Forms

| Source form | CAST variant | What it does |
|---|---|---|
| `import a.b [as x] [{ f, g }]` | `CrushModule { module_path, alias, selective }` | loads `a/b.crush` at compile time (below) |
| `use @lang python "m" [{ … }] [as x]` | `PolyglotModule` | written into each later `@lang` block of that language (CRUSH-224, unchanged) |
| `use @cap "fs.read" [{ … }] [as x]` | `Capability` | adds the manifest permission (unchanged) |
| `import @http/@git/@file "…" [as x]` | `External` | **compile error**: not supported yet |
| `use @mcp "url" [{ … }] [as x]` | `MCPImport` | **compile error**: not supported yet |

`SecureEnv` exists in CAST (no parser produces it) and keeps its old lowering.

## Resolution rules

- **Search path.** `import a.b` looks for `a/b.crush` in: the importing file's directory, then each
  `-L DIR` (repeatable, in order), then each `CRUSH_PATH` directory (colon-separated). First hit wins.
  There is no `a/b/mod.crush` form. Not found is a compile error listing every directory searched.
- **Mangling.** A module's functions are merged as `<module path, `.`→`::`>::<name>`:
  `facet::facet_parse`, `util::strings::title`. `::` is what lifted methods already use
  (`Point::norm2`), so the VMs, CASM, both AOT backends (`crush_aot::names::mangle`) and the debugger
  already take it; it cannot occur in a user-written identifier. `.` cannot be used: the compiler reads a
  dotted call name as a capability call on a receiver. A lifted method with the same spelling (a
  `struct facet` with a method `facet_parse`) is a compile error rather than a silent overwrite
  (`cosm_lower`). A second *file* imported under an already-used module path gets `#2`, `#3`, … on its
  prefix.
- **Binding.** `import a.b` binds the qualifier `b` (last segment); `as x` binds `x` instead. `q.f(…)`
  calls the module's `f` (also `q.f` as a function value); a name the module lacks is a compile error
  listing what it does define. `import a.b { f, g }` binds the bare names `f`, `g` — and the qualifier
  only if `as x` is also given. A selective name colliding with a function of the importing file, or with
  another selective import of a different function, is a compile error naming both. Two modules may export
  the same name; two qualifiers of the same spelling for different files are an error.
- **Inside a module** a bare call resolves to the module's own functions first
  (`__facet_list(…)` → `facet::__facet_list`), then the module's own selective imports, then globals
  (builtins, capabilities). A parameter or local of the same name shadows all of them.
- **Module contents.** Definitions only: `fn`, `struct` / `impl` / `trait` (incl. `@kind`), and imports
  of other Crush modules. The module's `main` is ignored. Any other top-level statement — `let`, a call,
  `use @…` — is a compile error naming file and line: a module has no init code, so there is no hidden
  init order. (`Parser::parse_module` keeps the top-level statements apart for this check.) Kind names
  (`struct` / `trait`) are **not** mangled: Kinds are one global namespace.
- **Graph.** Each file is loaded once per compile (keyed by canonical path, so a diamond shares one
  copy); a cycle is a compile error showing it (`ca.crush → cb.crush → ca.crush`); nesting is capped at
  `ImportOptions::max_depth` (64).
- **Placement.** Imports go at the top level of a file. One inside a function or block is refused.

## Diagnostics

- Loading errors (not found, cycle, bad name, collision, top-level statement, syntax error in a module)
  are `import_system::ImportError { file, pos, message }`; `Display` is `file:line:col: message`; JSON
  code **`E-IM01`** with the file the problem is in.
- Semantic errors inside imported code carry the module's file: every merged function records
  `meta["source_file"]`, and the checker attaches it (`SemanticError::file`, new). Text output becomes
  `errmod.crush:3:16: Undefined variable: …`; the `E-TP01` JSON record's `file` is the module's.
- The import statement's own position is now the `import` keyword (it used to be stamped after the
  statement, i.e. on the next line).

## API

| Caller | Use |
|---|---|
| compile a file | `crush_frontend::parse_file(path, &ImportOptions)`, or in the SDK `compile::compile_crush_file[_to_casm](path, &imports, &CompileOptions)` / `compile_crush_file_for_debug` |
| source with a location | `crush_frontend::parse_source_with_imports(src, origin, &opts)` (`origin` need not exist; only its directory is searched) |
| an already-parsed program | `import_system::resolve_imports(&mut program, origin, &opts)` |
| search path | `ImportOptions::new(lib_paths)` (only those) / `ImportOptions::with_crush_path(lib_paths)` (+ `CRUSH_PATH`; what the CLIs use) |

`parse_source(&str)` and every string entry point (`compile_crush_source`, `compile_crush_to_casm*`,
crush-web's `compile_crush_source_with`) resolve nothing; a program that still has a module import when
it is checked is refused with "`import x` was not resolved: … this compile has no file path to search
from". The same refusal stands in the compiler for CAST input (walker output included: Python/JS `import`
lowers to `CrushModule`, and is now a compile error rather than a `module.load` call that failed at run
time).

CLIs: `crushc`, `crush-run run` / `crush-run caps`, `crush-debug` and `crush-repl` take `-L/--lib-path DIR`
and read `CRUSH_PATH`; `crush run/build/debug/repl` forward to them. `crush-compile` assembles CASM only
and has no imports. The REPL resolves from its working directory and keeps earlier `import` lines in scope
for later lines. `crushc --emit ast` dumps the file as written (imports unresolved).

## Not yet

- **crush-pkg dependencies as search roots.** crush-pkg should resolve its dep graph to directories and
  pass them as `ImportOptions::search_paths` (ahead of `CRUSH_PATH`) to `compile_crush_file*`.
- **Remote / external imports** (`import @http/@git/@file`) and **`use @mcp`**: refused at compile time.
- **`use @lang` / `use @cap` inside an imported module**: refused (a top-level statement).
- **Top-level constants** in modules: write a function (`fn pi() { return 3.14159 }`).
- **Struct literals of an imported struct in the importer** (`Point { x: 1 }`): the parser recognises a
  struct literal only for structs declared in the same file; build values through module functions.
- **Source maps** of imported functions carry the module's line numbers but not its file, so a debugger
  break inside one shows the importer's file name.
- **Visibility**: every function of a module is importable (no `pub`/private distinction; the `__` prefix
  is only a convention, hidden from "it defines: …" lists).
