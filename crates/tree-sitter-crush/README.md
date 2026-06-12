# tree-sitter-crush

Tree-sitter grammar for the CRUSH programming language.

## Purpose

Provides a Tree-sitter grammar for parsing CRUSH source code. Used by:
- `crush_walker` for CRUSH → CAST transformation
- Code editors for syntax highlighting
- Development tools for code analysis

## Grammar Features

- Function definitions with parameters and return types
- Variable declarations and assignments
- Control flow (if/else, while, for)
- Capability calls
- Import/export statements
- Struct definitions
- Comments

## Usage

### In Rust

```rust
use tree_sitter::Parser;

let mut parser = Parser::new();
parser.set_language(tree_sitter_crush::language()).unwrap();

let source_code = "fn main() { print(\"Hello\") }";
let tree = parser.parse(source_code, None).unwrap();
```

### In Editors

For syntax highlighting in editors, see the `queries/` directory for highlight queries.

## Development

```bash
# Generate parser
tree-sitter generate

# Test grammar
tree-sitter test

# Parse example
tree-sitter parse examples/hello.crush
```

## See Also

- [`crush_walker`](../crush_walker/README.md) - Uses this grammar
- [Tree-sitter Documentation](https://tree-sitter.github.io/tree-sitter/)
