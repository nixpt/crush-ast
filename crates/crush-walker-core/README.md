# walker-core

Base utilities and traits for implementing language walkers in CRUSH.

## Purpose

`walker-core` provides the foundational infrastructure for building language walkers that transform source code from various programming languages into CRUSH's Abstract Syntax Tree (CAST) format. It offers:
- **Walker Trait**: Standard interface all language walkers must implement
- **BaseWalker**: Utility functions for common AST operations
- **Helper Methods**: String extraction, literal parsing, metadata creation

This crate enables CRUSH's polyglot capabilities by providing a consistent framework for adding new language support.

## Architecture

```
┌──────────────────────────────────────────────────┐
│              walker-core                         │
│                                                  │
│  ┌────────────────────────────────────────────┐ │
│  │  Walker trait                              │ │
│  │  - language() -> tree_sitter::Language     │ │
│  │  - walk(tree, source) -> ast::Program     │ │
│  └────────────────────────────────────────────┘ │
│                                                  │
│  ┌────────────────────────────────────────────┐ │
│  │  BaseWalker                                │ │
│  │  - text(node) -> &str                      │ │
│  │  - extract_string_literal()                │ │
│  │  - extract_int_literal()                   │ │
│  │  - extract_float_literal()                 │ │
│  │  - create_meta(node) -> metadata           │ │
│  └────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────┘
                      ▲
                      │ used by
         ┌────────────┴────────────┐
         │                         │
   ┌─────────────┐          ┌─────────────┐
   │python_walker│          │ rust_walker │
   └─────────────┘          └─────────────┘
```

## Key Components

### Walker Trait

The core interface all language walkers must implement:

```rust
pub trait Walker {
    /// Returns the tree-sitter Language for this walker
    fn language(&self) -> tree_sitter::Language;
    
    /// Transforms a parsed tree into CRUSH AST
    fn walk(&self, tree: &tree_sitter::Tree, source: &[u8]) -> Result<crush_cast::ast::Program>;
}
```

### BaseWalker

Utility struct providing common operations for tree-sitter node processing:

- **`text(node)`**: Extract UTF-8 text from a node
- **`child_text(node, field)`**: Get text from a named child field
- **`unwrap_parens(node)`**: Remove parentheses wrapping
- **`extract_string_literal(node)`**: Parse string literals (removes quotes)
- **`extract_int_literal(node)`**: Parse integer literals
- **`extract_float_literal(node)`**: Parse floating-point literals
- **`extract_bool_literal(node)`**: Parse boolean literals
- **`create_meta(node, lang, file)`**: Generate metadata (line, column, file, language)

## Relationships

### Dependencies

- **`tree-sitter`**: Parsing library for all language walkers
- **`crush-cast`**: CAST AST definitions
- **`anyhow`**: Error handling

### Dependents

All language walkers depend on this crate:
- **`python_walker`**: Python → CAST
- **`rust_walker`**: Rust → CAST
- **`bash_walker`**: Bash → CAST
- **`c_walker`**: C → CAST
- **`go_walker`**: Go → CAST
- **`js_walker`**: JavaScript → CAST
- **`zig_walker`**: Zig → CAST
- **`wasm_walker`**: WebAssembly → CAST

## Usage Example

Implementing a new language walker:

```rust
use walker_core::{Walker, BaseWalker};
use crush_cast::ast;
use anyhow::Result;

pub struct MyLangWalker;

impl Walker for MyLangWalker {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_mylang::language()
    }

    fn walk(&self, tree: &tree_sitter::Tree, source: &[u8]) -> Result<crush_cast::ast::Program> {
        let base = BaseWalker::new(source);
        let root = tree.root_node();
        
        let mut statements = vec![];
        
        for child in root.children(&mut root.walk()) {
            match child.kind() {
                "function_declaration" => {
                    let name = base.child_text(child, "name")?;
                    let meta = base.create_meta(child, "mylang", "input.mylang");
                    
                    statements.push(ast::Statement::FunctionDef {
                        name: name.to_string(),
                        params: vec![],
                        body: vec![],
                        meta,
                    });
                }
                _ => {}
            }
        }
        
        Ok(ast::Program {
            statements,
            meta: base.create_meta(root, "mylang", "input.mylang"),
        })
    }
}
```

## Metadata Format

All CAST nodes include metadata for error reporting and debugging:

```rust
{
    "line": 42,          // 1-indexed line number
    "column": 10,        // 1-indexed column number
    "file": "main.py",   // Source file name
    "lang": "python"     // Source language
}
```

This metadata is preserved through compilation and used by the VM for runtime error reporting.

## Design Principles

1. **Language-Agnostic**: Core utilities work for any tree-sitter grammar
2. **Error Preservation**: Metadata tracks source location through entire pipeline
3. **Minimal API**: Small, focused trait for easy implementation
4. **Reusable Utilities**: Common operations extracted to BaseWalker

## Walker Implementation Checklist

When implementing a new walker:

1. ✅ Add tree-sitter grammar dependency to `Cargo.toml`
2. ✅ Implement `Walker` trait
3. ✅ Use `BaseWalker` for common operations
4. ✅ Map language constructs to CAST nodes
5. ✅ Preserve metadata on all nodes
6. ✅ Handle unsupported features gracefully
7. ✅ Add tests with example source files
8. ✅ Document supported features in the walker's README

## Development

```bash
# Build
cargo build

# Test
cargo test

# Check documentation
cargo doc --no-deps --open
```

## See Also

- [`crush-cast`](../crush-cast/README.md) - CAST AST definitions
- [`python_walker`](../python_walker/README.md) - Example walker implementation
- [The Crush Language Guide](https://github.com/nixpt/crush-language-guide) - Full language documentation
