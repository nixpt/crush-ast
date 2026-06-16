# crush-python — Python bindings for Crush

Python bindings (via PyO3) for [crush-cast](../crush-cast/), the Crush
Abstract Syntax Tree IR. Allows Python code to parse, validate, and
inspect CAST JSON without maintaining a separate implementation.

## Build

```bash
# Install maturin
pip install maturin

# Build and install in the current venv
maturin develop --release

# Or build a wheel
maturin build --release
```

## Usage

```python
import crush

print(crush.cast_version())  # "0.2"

# Parse and validate a CAST program
program = crush.parse_cast('{"cast_version": "0.2", ...}')
valid = crush.validate_cast('{"cast_version": "0.2", ...}')
```

## Integration with Chroma

Replace chroma's native CAST→CVM1 lowerer (`chroma/crush/lower.py`)
with a subprocess call to `crushc --emit vm`, or use these bindings
for CAST validation and inspection directly from Python.
