# CAST Python Bindings

`cast_types.py` provides `@dataclass`-decorated classes that mirror the Crush AST
(CAST) so Python consumers can author programs without writing raw dicts.

## Quick start

```python
from dataclasses import asdict
import json
from cast_types import Program, Function, VarDecl, ExprStmt, Call, StringLiteral, Return

program = Program(
    cast_version="1.0",
    entry="main",
    lang="crush",
    functions={
        "main": Function(
            params=[],
            body=[
                VarDecl(
                    name="message",
                    value=StringLiteral(value="Hello, CAST!"),
                ),
                ExprStmt(
                    expr=Call(
                        function="io.print",
                        args=[Var(name="message")],
                    ),
                ),
                Return(value=None),
            ],
        ),
    },
)

# Serialize to JSON for the Rust compiler
json_str = json.dumps(asdict(program), indent=2)
print(json_str)
```

## Regenerating

If the Rust AST types change, regenerate the Python file:

```bash
cargo run -p crush-cast --bin export-py
```

The output is written to `python/cast_types.py` and should be committed.

## Type mapping

| Rust                  | Python                            |
|-----------------------|-----------------------------------|
| `struct`              | `@dataclass`                      |
| `enum` (tagged)       | `Union` of `@dataclass` variants  |
| `enum` (externally)   | `Union` of literals + wrappers    |
| `Option<T>`           | `Optional[T]`                     |
| `Vec<T>`              | `List[T]`                         |
| `HashMap<String, V>`  | `Dict[str, V]`                    |
| `serde_json::Value`   | `Any`                             |

## Round-trip guarantee

Building a `Program` in Python, calling `dataclasses.asdict`, and passing the
result through `json.dumps` produces JSON that `serde_json` on the Rust side
parses into an identical AST. This is enforced by the test suite in
`tests/python_roundtrip_tests.rs`.
