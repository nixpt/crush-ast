# Import system — what is specified, what is implemented

**Status:** design note, 2026-10-07 (CRUSH-173). Written from the code at `main` + lane E; open work is
**CRUSH-110** ("import is a no-op"). Line numbers are as of this commit.

## Summary

Crush has a full import *syntax* — five forms, parsed by both the hand-written parser and the
tree-sitter grammar, carried in CAST as `ImportStatement`. It has **no import semantics yet**:
nothing loads a module, nothing resolves a name across files, and the compiler lowers every form to a
runtime capability call that no host registers. The one thing that works end to end is the parse →
render round trip.

```text
$ cat a.crush
import helpers
fn main() { io.print("hi") }
$ crush-run run a.crush
[runtime] unknown capability: module.load        # exit 1, even though helpers is never used
```

## Syntax (implemented)

Parser: `crates/crush-frontend/src/parser/mod.rs` (`parse_import_statement`, ~1923). Grammar:
`crates/tree-sitter-crush/grammar.js` (~30–82). CAST: `crush_cast::ImportStatement`
(`crates/crush-cast/src/lib.rs` ~443), wrapped in `Statement::Import`.

| Source form | CAST variant |
|---|---|
| `import a.b.c [as X] [{ x, y }]` | `CrushModule { module_path, alias, selective }` |
| `import @http "…" [as X]` / `@git` / `@file` | `External { uri, resource_type, alias }` |
| `use @mcp "url" [{ "tool", … }] [as X]` | `MCPImport { server_url, tools, alias }` |
| `use @cap "fs.read" [{ "perm", … }] [as X]` | `Capability { capability_path, permissions, alias }` |
| `use @lang python "module" [{ "name", … }] [as X]` | `PolyglotModule { language, module_path, alias, selective }` |

Not accepted: `import "file.crush"` (a string after `import` is a parse error), `from … import`, any
other `use @…`. `ImportStatement::SecureEnv` exists in CAST and the compiler handles it, but **no parser
produces it** — `import secrets { … }` parses as a `CrushModule` named `secrets`. The Python and JS
walkers also emit `CrushModule` for their own `import` statements.

## Lowering (implemented, but nothing services it)

`crates/crush-frontend/src/compiler.rs` (~883–1069) turns each variant into instructions; no variant
consults a resolver:

| Variant | Emitted | Manifest permission |
|---|---|---|
| `CrushModule` | `push_str path; cap_call module.load 1; store alias` (`selective` dropped) | — |
| `Capability` | `push_str; cap_call cap.acquire; store` (`permissions` dropped) | `capability_path` |
| `PolyglotModule` | `exec_lang` running `import <path>` with no variables; `store` | — |
| `SecureEnv` | `secrets.read` per key, or `secrets.load_all` | `secrets.read` |
| `MCPImport` | `mcp.connect`, then `mcp.get_tool` per tool | `mcp.client` |
| `External` | `push_str uri; cap_call external.load; store` (`resource_type` dropped) | `external.load` |

None of `module.load`, `cap.acquire`, `external.load`, `mcp.connect`, `secrets.*` is registered by
`HostCapsBuilder` (`crates/crush-lang-sdk/src/host_caps.rs`) or built into the VM, so every program with
an import fails when the import statement executes — before any imported name is touched. The comment
at `compiler.rs` ~885 ("the ImportResolver handles resolution at runtime via capabilities") describes
an intent, not the code.

## Name resolution (not implemented)

- `SemanticAnalyzer::check` (`crates/crush-frontend/src/semantics.rs`) collects definitions from one
  `Program`; `Statement::Import` falls into the `_ => { // TODO }` arm (~317). Imported names and
  aliases are never put in scope. (`helpers.greet()` still type-checks because field access on an
  unknown/`any` value is a dynamic lookup — it then fails at runtime as above.)
- There is no multi-file merge on the compile path. `crushc -L/--lib-path` is parsed but only echoed
  under `-v` (`crates/crush-lang-sdk/src/bin/crushc.rs` ~67, ~147).

## `import_system.rs` (exists, never called)

`crates/crush-frontend/src/import_system.rs` defines `ImportResolver::resolve_import` with a branch per
variant, but no compile path, binary or SDK function calls it — only its own unit tests. What it does
when called:

- Crush modules: lookup in a hard-coded registry of three names (`io`, `fs`, `net`) with export *names*
  only, no bodies. No file reading, search path or extension handling.
- MCP / external / capability: string checks (scheme prefix, trusted-domain list, a dot in the path);
  no network, no git clone (`cloned: false`).
- `SecureEnv`: handles with `value: None`.
- `SecurityPolicy.max_import_depth` and the `allow_*` flags are never checked;
  `ImportError::ImportCycle` is declared and never produced.

`crates/crush-frontend/src/polyglot_imports.rs` builds on its types and is likewise unused.

## Specified vs implemented

| Feature | Where it is promised | State |
|---|---|---|
| Module syntax (`a.b`, alias, selective list) | grammar + parser | parsed; `selective` ignored by the compiler |
| Loading another `.crush` file | CRUSH-110; top-level `stdlib/README.md` ("loaded at compile time when a Crush program uses `import <module>`") | **no** |
| Search paths | `crushc -L` | flag only |
| Imported names visible to the type checker | CRUSH-110 | **no** |
| Cycle detection, depth limit, import policy | `import_system.rs` types | declared, never enforced |
| Runtime services (`module.load`, `external.load`, `mcp.connect`, `cap.acquire`, `secrets.*`) | compiler output | **none registered** — runtime error |
| Python/JS `import` inside `@python { }` / `@javascript { }` | `EXEC_LANG` | works — that is the guest language's own import, not Crush's |
| Stdlib access | — | works without `import`: `str.*`, `math.*`, … are host capabilities called by name |

## Direction (for CRUSH-110, not decided here)

What the code already makes cheap, and what it implies:

1. **Resolve at compile time, not run time.** A Crush module is CAST; loading it is "parse another file
   and merge its `functions` into the `Program`" before `SemanticAnalyzer::check`. That makes imported
   names type-check, needs no new capability, and keeps the import graph out of the VM's authority
   model. `module.load` as a runtime cap would let a program pull code by name at run time — that is an
   authority the capability model should grant explicitly, if ever.
2. **One resolver.** Either wire `ImportResolver` into `parse_source`/`compile_cast*` and give it real
   file loading (search path = importing file's directory, then `-L`), or delete it; today it is a
   second, unused description of the feature.
3. **Name mangling.** `functions` is a flat map; merged modules need a prefix (`helpers.greet` →
   `helpers::greet` or similar) so two modules can export the same name, and the parser's
   `a.b()` call shape has to resolve to it.
4. **The non-Crush forms stay capabilities.** `use @cap` is already the right shape (it adds a manifest
   permission); `use @mcp` / `import @http` are host services and belong behind named grants like any
   other I/O. They should fail at compile time with "not supported" until a host implements them,
   instead of compiling to calls that can only fail.
5. **Tests that run.** The only import tests today check emitted bytecode or round-trip rendering; the
   conformance example `crates/tree-sitter-crush/test_imports.crush` is annotated
   `expect-error: unknown capability: module.load`. A real import test is two files and a `crush-run`.
