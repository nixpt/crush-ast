# tree-sitter-crush

Tree-sitter grammar for the CRUSH programming language.

## Purpose

Provides a Tree-sitter grammar for parsing CRUSH source code. Used by:

- Code editors for syntax highlighting
- Development tools for code analysis (`crush-symbols`, and `exosphere`'s
  embedded copy — the one path dependency between the two repos, CRUSH-23)

Note: `crush-frontend` in this workspace does **not** use this grammar. It has
its own hand-written lexer and parser (`crates/crush-frontend/src/parser/`),
which is the source of truth for the language. This grammar tracks it, and
`tests/grammar_examples.rs` fails if it drifts.

## Grammar Features

- Function definitions with parameters and return types (`fn f(n: Int) -> Int`)
- Variable declarations and assignments (`x = 1`, `p.x = 2`, `xs[0] = 3`)
- Control flow (if/else, while, for), `try`/`catch`, `match`, `semantic_switch`
- Capability calls and `capability <name> <access>` declarations
- Import/export statements, polyglot blocks (`@python { … }`)
- Struct definitions and struct/object literals (`Point { x: 1 }`, `{"k": 1}`)
- AI-native annotations (`@module`, `@decision`, `@invariant`, `@wip`, …)
- `//`, `#` and `/* … */` comments

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
# Regenerate src/parser.c + grammar.json + node-types.json after editing
# grammar.js. Needs tree-sitter CLI >= 0.25 (the committed parser.c is ABI 15).
tree-sitter generate

# Grammar corpus (test/corpus/*.txt) — needs the CLI, so CI doesn't run it.
tree-sitter test

# The check CI *does* run: parse every examples/crush/*.crush with the bundled
# parser and fail on ERROR nodes.
cargo test -p tree-sitter-crush
```

`tree-sitter test --update` refreshes the corpus expectations but drops field
names for new cases; `tree-sitter parse` prints them, so prefer re-emitting
from that when adding cases.

## See Also

- [Tree-sitter Documentation](https://tree-sitter.github.io/tree-sitter/)
- [The Crush Language Guide](https://github.com/nixpt/crush-language-guide) - Full language documentation
