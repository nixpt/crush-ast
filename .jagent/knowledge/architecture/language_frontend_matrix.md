# Crush-AST Language Frontend Matrix

> **Last updated:** 2026-06-24
> **Workspace:** All 82+ VM tests pass, 21+ frontend tests per language pass, 0 build errors.

---

## Implementation Status

| Language | Trait | Parser | Source Positions | Tests | Binary | Status |
|----------|-------|--------|-----------------|-------|--------|--------|
| **JavaScript** | `Frontend` | swc 41 (primary) + boa 0.21 (optional) | ✅ Span→byte→line/col | 14 | `js_walker` | ✅ Complete |
| **Python** | `Frontend` | rustpython-parser 0.4 | ✅ Ranged→byte→line/col | 9 | `python_walker` | ✅ Complete |
| **Bash** | `Frontend` | brush-parser 0.4 | ✅ SourceLocation→direct l/c | 21 | `bash_walker` | ✅ Complete |
| **Rust** | `Frontend` | syn 2.0 | ⚠️ File/lang only (syn opaque) | 8 | `rust_walker` | ✅ Complete |
| **Go** | `Walker`→`TreeSitterFrontend` | tree-sitter-go 0.23 | ✅ tree-sitter create_meta | 2 | `go_walker` | ✅ Complete |
| **C** | `Walker`→`TreeSitterFrontend` | tree-sitter-c 0.23 | ✅ tree-sitter create_meta | 4 | `c_walker` | ✅ Complete |
| **Zig** | `Walker`→`TreeSitterFrontend` | tree-sitter-zig 1.1 | ✅ tree-sitter create_meta | 4 | `zig_walker` | ✅ Complete |
| **Zsh** | `Frontend` | zshrs-parse 0.10 | ❌ No (source not bundled) | 14 | `zsh_walker` | ⚠️ Needs source bundle |
| **WASM** | *(none)* | wasmparser 0.121 | ❌ No (file/lang only) | 0 | *(implicit)* | 🔴 Proof-of-concept |
| **C++** | *(none)* | Delegated to `c_walker` | N/A | 0 | N/A | 🔴 Partial — no cpp grammar |
| **Crush** | *(native)* | Self-hosted parser | N/A | — | `crush-compile` | ✅ Internal |

---

## Feature Completeness per Language

### JavaScript (`crush-lang-js`)
**Parser:** swc_ecma_parser 41 (default) / boa_parser 0.21 (optional `boa-backend` feature)

| Feature | Status | Notes |
|---------|--------|-------|
| Function declarations | ✅ |  |
| Variable declarations | ✅ | var, let, const |
| Expressions (binary, unary) | ✅ |  |
| String/number/boolean literals | ✅ |  |
| Call expressions | ✅ |  |
| Class declarations | ✅ |  |
| Arrow functions | ✅ |  |
| Object literals | ✅ |  |
| Array literals | ✅ |  |
| Template literals | ✅ |  |
| Module import/export | ✅ |  |
| Return/if/while/for | ✅ |  |
| Capability detection | ✅ | `meta()` via swc Span positions; centralized `map_to_capability()` for known calls |
| Source positions | ✅ | swc nodes have `.span: Span { lo: BytePos }` — extracted in `meta(span, ctx)` |
| Boa backend | ✅ | Feature-gated (`boa-backend`). Lower source positions via `meta0()` fallback |
| **Known gaps** | | Try/catch, generators, async/await, regex literals not yet lowered |

### Python (`crush-lang-python`)
**Parser:** rustpython-parser 0.4 / rustpython-ast 0.4

| Feature | Status | Notes |
|---------|--------|-------|
| Function definitions | ✅ | `def` with params |
| Variable assignments | ✅ | Simple, augmented, attribute, subscript targets |
| Expressions (binary, unary) | ✅ |  |
| Constants (int, float, string, bool, None) | ✅ |  |
| Call expressions | ✅ | With capability mapping (print→io.print, len, int/str/float/...) |
| If/elif/else | ✅ |  |
| While loops | ✅ |  |
| For loops | ✅ |  |
| Break/continue | ✅ |  |
| Lists, tuples, dicts | ✅ |  |
| Imports | ✅ | `import X`, `from X import Y` with dangerous-import detection |
| Source positions | ✅ | `Ranged::start()` → byte offset → `ctx.meta_at()` |
| Feature analysis | ✅ | `FeatureReport`: detects async, classes, exceptions, dangerous imports |
| **Not lowered** | ❌ | Comprehensions, generators/yield, async/await, with, try/except, class defs, match, global/nonlocal, lambda, del, assert, type aliases |
| **Known gaps** | | F-string interpolation not supported; multi-target assignment not supported |

### Bash (`crush-lang-bash`)
**Parser:** brush-parser 0.4

| Feature | Status | Notes |
|---------|--------|-------|
| Simple commands | ✅ | Full `word_to_expr()` with `$VAR`, `${VAR}`, quoted strings, string concatenation |
| echo/printf | ✅ | Mapped to `io.print` capability |
| read | ✅ | Mapped to `io.readline` capability |
| cat/head/tail/wc/sort/grep | ✅ | Mapped to `fs.read` capability |
| local | ✅ | Variable declarations |
| exit/return | ✅ | Mapped to `Return` |
| export | ✅ | Mapped to `Export` |
| source/. | ✅ | Mapped to `bash.source` capability |
| cd | ✅ | Mapped to `env.set` capability |
| If clauses | ✅ | With elif/else |
| While/until loops | ✅ |  |
| For loops | ✅ | With `in` word list |
| Case statements | ✅ | With or-patterns |
| Brace groups, subshells | ✅ |  |
| Arithmetic commands | ✅ | Mapped to `bash.arithmetic` capability |
| Arithmetic for loops | ✅ | Mapped to `bash.arithmetic_for` capability |
| Variable references in strings | ✅ | `extract_var_refs()` with `$VAR`, `${VAR}` in double-quoted strings |
| Heredocs | ✅ | Tested: does not crash |
| Source positions | ✅ | `SourceLocation::location()` → direct `(line, column)` → `ctx.meta_lc()` |
| Feature analysis | ✅ | `FeatureReport`: detects functions, commands, side effects, dangerous commands |
| **Not lowered** | ❌ | Test command evaluation (partial), function params, select loops, coprocess |
| **Known gaps** | | `word_to_expr` returns `StringLiteral` for complex words — some context lost |

### Rust (`crush-lang-rust`)
**Parser:** syn 2.0 (features: `full`, `extra-traits`)

| Feature | Status | Notes |
|---------|--------|-------|
| Function definitions | ✅ | Name, params, body |
| Variable declarations (`let`) | ✅ | With optional init |
| Expressions (binary, unary) | ✅ | Arithmetic, comparison, logical, bitwise |
| Literals (int, float, string, bool, char) | ✅ |  |
| If/else | ✅ | Statement + expression forms |
| Return | ✅ |  |
| Call expressions | ✅ | println/print→`io.print`; len→intrinsic |
| Macros (println!/print!) | ✅ | Basic support via Stmt::Macro |
| Block expressions | ✅ | `__crush_let__`, `__crush_return__` wrappers |
| Source positions | ⚠️ | `LowerCtx` threaded through lowering, but syn's `proc_macro2::Span` provides no usable positions outside a proc-macro context. Only file/lang metadata available. |
| Feature analysis | ✅ | `FeatureReport`: detects unsafe, FFI, imports |
| **Not lowered** | ❌ | Structs, traits, enums, impl blocks, closures, pattern matching, loops (for/while/loop), references/deref, method calls, generics, attributes, modules |
| **Known gaps** | | Heavy use of `anyhow::bail!("unsupported...")` — many constructs fall through. Items other than `fn` and `Use` are unsupported. |

### Go (`go_walker`)
**Parser:** tree-sitter-go 0.23

| Feature | Status | Notes |
|---------|--------|-------|
| Function declarations | ✅ | With params, body |
| Short var declarations (`:=`) | ✅ |  |
| Call expressions | ✅ | Capability mapping via `map_to_capability("go", ...)` |
| If/else | ✅ |  |
| Return | ✅ |  |
| Basic expressions | ✅ | Binary ops, int/float/string/bool/nil literals, identifiers, selectors |
| `__crush_export__` | ✅ | Special call detection |
| Source positions | ✅ | `BaseWalker::create_meta(node, "go", file_name)` |
| cast_version | ⚠️ | `"0.1"` — should be `"0.2"` to match others |
| **Not lowered** | ❌ | Packages, imports, structs, interfaces, methods (only functions), goroutines, channels, defer, range loops (only `for`), slices, maps, type switches, select statements |
| **Known gaps** | | The Walker struct is in main.rs (not lib.rs) so it can't be imported as a library. No `[[bin]]` in Cargo.toml. |

### C (`c_walker`)
**Parser:** tree-sitter-c 0.23

| Feature | Status | Notes |
|---------|--------|-------|
| Function definitions | ✅ |  |
| Variable declarations | ✅ |  |
| If/else | ✅ |  |
| While/for/do-while | ✅ |  |
| Return/break/continue | ✅ |  |
| Call expressions | ✅ | Capability mapping via `map_to_capability("c", ...)` |
| Basic expressions | ✅ | Binary ops, literals, identifiers |
| Source positions | ✅ | `BaseWalker::create_meta(node, "c", file_name)` |
| **Not lowered** | ❌ | Struct/union/enum declarations, pointers, arrays, string literals, goto, switch, preprocessor directives, type casts, sizeof, bitfields |
| **Known gaps** | | Same library-export problem as go_walker — main.rs only, no `[[bin]]`. 604 lines, most detailed of the tree-sitter walkers. |

### Zig (`zig_walker`)
**Parser:** tree-sitter-zig 1.1

| Feature | Status | Notes |
|---------|--------|-------|
| Function declarations | ✅ |  |
| Variable declarations | ✅ | `const` and `var` |
| If/else | ✅ |  |
| While/for | ✅ |  |
| Return | ✅ |  |
| Call expressions | ✅ | Capability mapping via `map_to_capability("zig", ...)` |
| Basic expressions | ✅ | Literals, identifiers, binary ops |
| Source positions | ✅ | `BaseWalker::create_meta(node, "zig", file_name)` |
| **Not lowered** | ❌ | Structs, enums, unions, comptime, error handling, slices, defer, switch, inline loops, packed structs, alignment, tests, async |
| **Known gaps** | | Has `[[bin]]` section. Walker is `pub struct`. Still no lib.rs though. |

### Zsh (`crush-lang-zsh`)
**Parser:** zshrs-parse 0.10

| Feature | Status | Notes |
|---------|--------|-------|
| (Similar to bash structure) | ⚠️ | Structurally similar to Bash frontend, 42 lines in lib.rs + analyzer/lowerer/parser |
| Source positions | **🔴 NONE** | `parse()` does NOT bundle source string. `lower()` does NOT use `LowerCtx`. All meta = `HashMap::new()`. |
| **Action needed** | | Bundle source in `parse()`, create `LowerCtx` in `lower()`, thread through lowering functions (same pattern as Bash) |

### WASM (`wasm_walker`)
**Parser:** wasmparser 0.121

| Feature | Status | Notes |
|---------|--------|-------|
| Module structure | ⚠️ | Basic: imports, exports, functions, memory, data segments |
| Function bodies | ⚠️ | Very limited — primarily metadata extraction |
| Source positions | **🔴 NONE** | `HashMap::from([("file",..),("lang",..)])` — no line/column |
| **Action needed** | | Implement `Walker` or `Frontend` trait. Add `[[bin]]` to Cargo.toml. This is the only walker without either trait. Wasm binaries have no source position concept — use 0,0. |
| cast_version | ⚠️ | `"0.1"` — should be `"0.2"` |

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────┐
│                  Subprocess CLI                       │
│            cli/src/main.rs (walker bin)               │
│   maps extension → walker binary, calls as subprocess │
└──────────────────────┬──────────────────────────────┘
                       │ JSON CAST on stdout
         ┌─────────────┼─────────────┐
         ▼             ▼             ▼
  ┌──────────┐  ┌──────────┐  ┌──────────┐
  │ js_walker│  │py_walker │  │bash_...  │  ... 7 more binaries
  └────┬─────┘  └────┬─────┘  └────┬─────┘
       │              │              │
  ┌────▼─────┐  ┌────▼─────┐  ┌────▼─────┐
  │Frontend  │  │Frontend  │  │Frontend  │
  │ impl     │  │ impl     │  │ impl     │
  └──────────┘  └──────────┘  └──────────┘

In-process alternative:
┌───────────────────────────────────────────────┐
│         frontend_pipeline(&frontend, source)   │
│  ┌──────┐   ┌─────────┐   ┌──────┐            │
│  │parse │→  │analyze  │→  │lower │→ CAST      │
│  └──────┘   └─────────┘   └──────┘            │
└───────────────────────────────────────────────┘

Tree-sitter bridge:
┌────────────────────────────────────┐
│ TreeSitterFrontend<W: Walker>       │
│  → implements Frontend              │
│  → wraps c_walker/go_walker/zig     │
└────────────────────────────────────┘
```

---

## Test Counts by Crate

| Crate | Unit | Integration | Pipeline (VM) | Total |
|-------|------|-------------|---------------|-------|
| walker-core | 0 | 1 (doc-test) | — | 1 |
| go_walker | 2 | — | — | 2 |
| c_walker | 4 | — | — | 4 |
| zig_walker | 4 | — | — | 4 |
| crush-lang-js | 0 | 12 | 2 | 14 |
| crush-lang-python | 0 | 6 | 3 | 9 |
| crush-lang-rust | 0 | 5 | 3 | 8 |
| crush-lang-bash | 0 | 16 | 5 | 21 |
| crush-lang-zsh | 0 | 14 | 0 | 14 |
| **Total** | | | | **77** |

---

## Priority Action Items

1. **🟢 Fix Zsh frontend** — Bundle source in `parse()`, use `LowerCtx` in `lower()`. Bash frontend is the reference pattern (~15 min).
2. **🟢 Fix cast_version** — `go_walker` and `wasm_walker` produce `"0.1"` instead of `"0.2"` (~2 min).
3. ✅ **Extract Walker structs into lib.rs** — All three tree-sitter walkers (c, go, zig) now have `lib.rs` exporting the `Walker` impl + thin `main.rs` using `run_walker_binary()`. Template below for new walkers.
4. **🟡 Implement trait for wasm_walker** — Either `Walker` or `Frontend` (~30 min).
5. **🟡 Address C++ gap** — Add `tree-sitter-cpp` grammar crate and walker for C++ support, or document that `c_walker` is C-only (~1-2 hrs).
6. **🔵 More Python coverage** — Comprehensions, async/await, try/except, class defs are the most requested missing constructs.
7. **🔵 More Rust coverage** — Loops, pattern matching, struct construction, method calls.
8. **🔵 More JavaScript coverage** — Try/catch, async/await, generators, regex literals.
9. **🔵 syn source positions** — Not fixable without nightly proc-macro or alternative parser.

### Legend
- 🟢 **Easy** — 1-15 min, well-understood pattern
- 🟡 **Medium** — 30-60 min, some design decisions
- 🔵 **Hard** — Hours+, significant parser/lowering work

---

## How to Add a New Tree-Sitter Walker

All three tree-sitter walkers (c, go, zig) follow the same structure after the lib.rs refactoring. To add a new one (e.g., Java):

### 1. Create `Cargo.toml`

```toml
[package]
name = "java_walker"
version = "0.1.0"
edition = "2021"
license.workspace = true
repository.workspace = true
description = "Java language walker — parses Java source into CAST IR"

[lib]
name = "java_walker"

[[bin]]
name = "java_walker"
path = "src/main.rs"

[dependencies]
crush-cast.workspace = true
tree-sitter.workspace = true
tree-sitter-java = "0.23"
clap.workspace = true
anyhow.workspace = true
serde.workspace = true
serde_json.workspace = true
walker-core.workspace = true
```

### 2. Create `src/lib.rs` — the Walker implementation

```rust
use anyhow::Result;
use crush_cast::{self as ast, CastType, Expression, Statement};
use serde_json::json;
use std::collections::HashMap;
use tree_sitter::{Node, Tree};
use walker_core::{BaseWalker, Walker};

pub struct JavaWalker {
    pub file_name: String,
}

impl Walker for JavaWalker {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_java::LANGUAGE.into()
    }

    fn walk(&self, tree: &Tree, source: &[u8]) -> Result<ast::Program> {
        let base = BaseWalker::new(source);
        let root = tree.root_node();
        // ... walk tree, produce CAST Program
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use crate::*;
    use tree_sitter::Parser;
    // ... tests using parse_and_walk helper
}
```

### 3. Create `src/main.rs` — thin subprocess binary

```rust
use anyhow::Result;
use clap::Parser as ClapParser;
use walker_core::run_walker_binary;

#[derive(ClapParser)]
#[command(name = "java_walker")]
struct Cli {
    input: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    run_walker_binary(
        java_walker::JavaWalker { file_name: cli.input.clone() },
        "java",
        &[".java"],
        &cli.input,
    )
}
```

### 4. Register in the workspace

Add `java_walker` to `[workspace.members]` in the root `Cargo.toml`. Add the extension mapping in `walker_core::frontend_for_extension()`.

### 5. Add `run_walker_binary()` helper

Already available in `walker-core` — handles file reading, `TreeSitterFrontend` setup, pipeline execution, and JSON output. Your `main.rs` is just CLI parsing + one function call.
