# CRUSH-110 — `import` is a no-op in the standalone toolchain

| Field | Value |
|-------|-------|
| **ID** | CRUSH-110 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M4 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | L |

## Problem

The `import` statement is unimplemented scaffolding. `crush-frontend` lowers
*any* `import X` to `cap_call "module.load"` (`compiler.rs` ~line 899), but no
capability registry registers `module.load` — not `crush-vm/src/caps.rs` (the
portable registry), not `crush-lang-sdk`'s host-cap builders (`--stdlib`,
`--fs`, …), nowhere in the standalone binary. As a result, importing either a
stdlib module (`import strings`) or a user file (`import helper`) defines
nothing, and running hits `[runtime] unknown capability: module.load`.

The `ImportResolver` (`import_system.rs`) only knows three hardcoded module
names (`io`, `fs`, `net`) whose `exports` are name-only maps with **no
function bodies**, and there is no file/URL loading path despite
`allow_file_imports` / `allow_git_imports` being true in its policy. There is
currently **no way to share native Crush functions across files**.

## Impact

Every program must be a single self-contained file. Multi-file projects,
libraries, and shared helpers are impossible in the default toolchain. This is
the direct reason the two interpreters in the `awesome-crush` collection
(`forth/`, `brainfuck/`) carry an identical `scan()` helper copy-pasted into
each file, with a comment explaining the import gap.

## Reproduction

```bash
# a user module is never loaded — the type checker never sees its fns
printf 'fn double(x) { return x * 2; }\n' > /tmp/helper.crush
printf 'import helper\nfn main() { io.print(double(21)); return 0; }\n' > /tmp/uses.crush
crushc /tmp/uses.crush -o /tmp/uses.cvm1
# [type] type error: Undefined function: double

# a stdlib module is never loaded either — runtime "unknown capability"
printf 'import strings\nfn main() { io.print(strings.to_uppercase("hi")); return 0; }\n' > /tmp/s.crush
crushc /tmp/s.crush -o /tmp/s.cvm1 && crush-run run /tmp/s.cvm1 --cap io.print --cap module.load
# [runtime] unknown capability: module.load
```

Note: `crates/tree-sitter-crush/test_imports.crush` already documents the same
failure as `// expect-error: [runtime] unknown capability: module.load`.

## Success criteria

- [ ] `import <file>.crush` makes that file's `fn`s callable from the importing program via the standalone `crushc`/`crush-run`.
- [ ] `import strings` (and the other `stdlib/` modules) resolves to working functions.
- [ ] No import path emits an unregistered `module.load` capability.
- [ ] Import cycles are detected and reported (`ImportError::ImportCycle` exists but is never produced).

## Technical approach

- Pick a model: **compile-time loading** (crushc reads + inlines the imported module's functions before the type checker runs) vs a runtime `module.load` host cap. Compile-time is simpler and matches `stdlib/README.md`'s own claim ("loaded at compile time").
- Give `ImportResolver` a real loader: resolve paths relative to the importing file, map `stdlib/` modules, or populate a module registry with actual function bodies (today `resolve_crush_module` only returns name bookkeeping).
- In `semantics.rs`, register imported functions into `self.functions` before body checking — right now they never land there, which is why the type checker reports "Undefined function".
- Remove or implement the `cap_call "module.load"` lowering in `compiler.rs` (~line 882–908).

## Files to modify

- `crates/crush-frontend/src/import_system.rs` — real module loading.
- `crates/crush-frontend/src/compiler.rs` — replace the `module.load` cap_call lowering.
- `crates/crush-frontend/src/semantics.rs` — register imported fns before type checking.
- `crates/crush-lang-sdk/src/bin/crushc.rs` — import-path resolution (relative to the compiled file, or a `-I`/module-path flag).

## Non-goals

- Network / git imports (the `allow_network_imports` / `allow_git_imports` policy), MCP imports, secure-env imports.
- Runtime hot-reload of modules.
- Sharing `@polyglot` blocks across files.
