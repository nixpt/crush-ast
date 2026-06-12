# CASM Program JSON Schema

This document describes the JSON wire format of `casm::Program`, the bytecode
representation consumed by the Crush VM.  It is the output of
`crush_lang::compile_cast()` and the input to `casm::Program::load()`.

## Top-level object: `Program`

| Field       | Type                    | Required | Description                                      |
|-------------|-------------------------|----------|--------------------------------------------------|
| `version`   | `string`                | yes      | CASM format version (e.g. `"1.0"`).              |
| `functions` | `map<string, Function>` | yes      | Named functions that make up the program.        |
| `manifest`  | `Manifest`              | no       | Capability permissions required at runtime.      |
| `lang`      | `string \| null`        | no       | Source language hint (e.g. `"crush"`, `"py"`).   |

### `Function`

| Field    | Type              | Required | Description                                          |
|----------|-------------------|----------|------------------------------------------------------|
| `params` | `array<string>`   | no       | Parameter names, in order.                           |
| `locals` | `array<string>`   | no       | Local variable names (populated by the VM).          |
| `body`   | `array<Instruction>` | yes   | Stack-based instructions.                            |

### `Instruction`

| Field          | Type     | Required | Description                                                     |
|----------------|----------|----------|-----------------------------------------------------------------|
| `op`           | `string` | yes      | Opcode name (e.g. `"push_int"`, `"call"`, `"cap_call"`).        |
| `args`         | `object` | no       | Opcode-specific arguments (flattened into the instruction).     |
| `instr_lang`   | `string` | no       | Source language tag for polyglot debugging.                     |
| `meta`         | `any`    | no       | Opaque debug/metadata (often a JSON object with `line`/`col`).  |

#### Common `args` shapes by opcode

| Opcode          | Expected `args` keys                                      |
|-----------------|-----------------------------------------------------------|
| `push_int`      | `{ "value": <i64> }`                                      |
| `push_float`    | `{ "value": <f64> }`                                      |
| `push_str`      | `{ "value": <string> }`                                   |
| `push_bool`     | `{ "value": <bool> }`                                     |
| `push_null`     | `{}`                                                      |
| `store`         | `{ "name": <string> }`                                    |
| `load`          | `{ "name": <string> }`                                    |
| `export_var`    | `{ "name": <string> }`                                    |
| `add` / `sub` / `mul` / `div` / `mod` / `neg` | `{}` |
| `eq` / `ne` / `lt` / `gt` / `le` / `ge`     | `{}` |
| `and` / `or` / `not`                        | `{}` |
| `jmp`           | `{ "target": <usize> }`                                   |
| `jmp_if`        | `{ "target": <usize> }`                                   |
| `jmp_if_not`    | `{ "target": <usize> }`                                   |
| `call`          | `{ "function": <string>, "argc": <usize> }`               |
| `ret`           | `{}`                                                      |
| `cap_call`      | `{ "name": <string>, "argc": <usize> }`                   |
| `call_host`     | `{ "capsule": <string>, "ic_id": <hex32>, "method": <string>, "argc": <usize> }` |
| `call_interface`| `{ "handle": <string>, "method": <string>, "argc": <usize> }` |
| `new_array`     | `{ "size": <usize> }`                                     |
| `arr_get` / `arr_set` / `arr_len` / `arr_push` / `arr_pop` | `{}` |
| `new_obj`       | `{}`                                                      |
| `new_struct`    | `{ "name": <string> }`                                    |
| `get_field`     | `{ "name": <string> }`                                    |
| `set_field`     | `{ "name": <string> }`                                    |
| `type_of`       | `{}`                                                      |
| `cast`          | `{ "type": <string> }`                                    |
| `exec_lang`     | `{ "lang": <string>, "code": <string>, "var_count": <usize>, "var_0"... }` |
| `str_contains` / `str_split` / `str_replace` / `str_join` | `{}` |
| `dom_query`     | `{ "query_type": <string> }`                              |
| `dom_mutate`    | `{ "mutation": <string>, "has_value": <bool>, "has_value2": <bool> }` |
| `dom_event_listener` | `{ "event": <string> }`                              |
| `enter_try`     | `{ "target": <usize> }`                                   |
| `exit_try`      | `{}`                                                      |
| `throw`         | `{}`                                                      |
| `spawn`         | `{}`                                                      |
| `yield`         | `{}`                                                      |
| `await`         | `{ "handle": <string> }`                                  |
| `len`           | `{}`                                                      |
| `index`         | `{}`                                                      |
| `make_range`    | `{}`                                                      |
| `dup` / `pop` / `swap` / `rot` | `{}`                                 |
| `pick` / `roll` | `{ "n": <usize> }`                                        |
| `break` / `continue` | `{}`                                                 |
| `bit_and` / `bit_or` / `bit_xor` / `bit_not` / `shl` / `shr` | `{}` |

### `Manifest`

| Field         | Type             | Required | Description                                   |
|---------------|------------------|----------|-----------------------------------------------|
| `permissions` | `array<string>`  | no       | Capability names the program is allowed to call.|

## Example

```json
{
  "version": "1.0",
  "lang": "crush",
  "functions": {
    "main": {
      "params": [],
      "locals": [],
      "body": [
        { "op": "push_int", "args": { "value": 42 } },
        { "op": "cap_call", "args": { "name": "io.print", "argc": 1 } },
        { "op": "push_null" },
        { "op": "ret" }
      ]
    }
  },
  "manifest": {
    "permissions": ["io.print"]
  }
}
```

## Crush text format (`.crush`)

CAST programs can be rendered to human-readable `.crush` source via
`crush_lang::render_program()` and parsed back via `crush_lang::parse_source()`.

### Whitespace canonicalization rules

When comparing `render(parse_source(text))` round-trips, the following
whitespace rules are normalized:

* **Indentation** is always 4 spaces per nesting level.
* **Blank lines** separate top-level function definitions; top-level statements
  in `main` have no blank lines between them.
* **No trailing whitespace** on any line.
* **Single trailing newline** at end of file.
* **Expressions** are parenthesized minimally — only when required by
  precedence.
* `else if` chains are collapsed without nested braces.
* `export name` (without `=`) is the canonical form when the value is the
  variable of the same name.
* `struct` and `match` expressions with simple bodies are rendered on a single
  line because the text parser does not currently accept newlines inside them.
* AI-native nodes (`Expression::AI`, `Statement::AI`) are rendered in a
  descriptive textual form prefixed with `# AI-NATIVE: read-only` because the
  text parser does not accept them.

## TypeScript Bindings

The CAST AST types are also available as TypeScript declarations for non-Rust
runtimes (web apps, Node tooling, desktop apps):

* **Bindings file:** [`bindings/cast.d.ts`](bindings/cast.d.ts) (committed, not gitignored)
* **Generation command:**
  ```bash
  cargo run -p crush-cast --bin export-ts --features ts-export
  ```

### Usage Example

```typescript
import type { Program, Statement, Expression, CastType } from './bindings/cast';

// Author a CAST program directly in TypeScript
const helloWorld: Program = {
  cast_version: "1.0",
  entry: "main",
  lang: "crush",
  functions: {
    main: {
      params: [],
      body: [
        {
          type: "ExprStmt",
          expr: {
            type: "Call",
            function: "io.print",
            args: [
              { type: "StringLiteral", value: "Hello from CAST!", meta: {} }
            ],
            meta: {}
          },
          meta: {}
        },
        {
          type: "Return",
          value: null,
          meta: {}
        }
      ],
      meta: {}
    }
  },
  ai_meta: null
};

// Validate with the TypeScript compiler
// tsc --noEmit my-script.ts
```

## Python Bindings

The CAST AST types are also available as Python `@dataclass` declarations for
Python-runtime agents (Joker MCP consumers, scripts, notebook explorations):

* **Bindings file:** [`python/cast_types.py`](python/cast_types.py) (committed, not gitignored)
* **Generation command:**
  ```bash
  cargo run -p crush-cast --bin export-py
  ```

### Usage Example

```python
from dataclasses import asdict
import json
from cast_types import Program, Function, ExprStmt, Call, StringLiteral, Return

hello_world = Program(
    cast_version="1.0",
    entry="main",
    lang="crush",
    functions={
        "main": Function(
            params=[],
            body=[
                ExprStmt(
                    expr=Call(
                        function="io.print",
                        args=[StringLiteral(value="Hello from CAST!")],
                    ),
                ),
                Return(value=None),
            ],
        ),
    },
)

# Round-trip: Python dataclass -> dict -> JSON -> Rust AST
json_str = json.dumps(asdict(hello_world))
```

### Design Notes

* **Tagged enums** (`Statement`, `Expression`, `AIExpression`, etc.) are modeled
  as separate `@dataclass` variants with a discriminator field (`type` or
  `ai_type`) and a `Union` type alias.
* **Externally tagged enums** (`CastType`, `ExternalResourceType`, etc.) use
  wrapper dataclasses (e.g. `_CastTypeArray`) so that `dataclasses.asdict`
  produces the same single-key-object shape that `serde` expects.
* **Unit enums** (`Priority`, `KnowledgeType`, etc.) are typed as
  `Literal[...]` string unions.
* The `Statement::Import` field is named `import_` in Python (to avoid the
  keyword conflict). Rust accepts both `import` and `import_` via
  `#[serde(alias = "import_")]`.

## Notes for agents

* `casm::Program` is **serde-serializable** — `serde_json::to_string_pretty` and
  `serde_json::from_str` are sufficient for round-tripping.
* The binary format (`.casmb`) is MessagePack with a shebang header; the JSON
  format (`.casm`) is the canonical human-readable wire format.
* Debug metadata (`meta`, `instr_lang`) is optional and ignored by the VM
  during execution, but `meta` objects containing `line`/`col`/`file` keys are
  used by `DebugInfo` to produce source-mapped error messages.
* The committed `bindings/cast.d.ts` must stay in sync with the Rust types.
  Run `cargo run -p crush-cast --bin export-ts --features ts-export` after
  modifying AST definitions and include the updated `.d.ts` in your commit.
