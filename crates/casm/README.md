# CASM - Crush Assembly

Low-level bytecode format for the Crush execution environment.

## Purpose

CASM (Crush Assembly) is the intermediate representation used by the Crush VM. It's the compilation target for all Crush source code and polyglot languages.

## Compilation Pipeline

```
Source Code (Crush/Python/JS/etc.)
       ↓
   Parser/Lexer
       ↓
   CAST (AST)
       ↓
   Compiler
       ↓
   CASM (Bytecode)  ← This crate
       ↓
   NanoVM
```

## Instruction Set

### Stack Operations
- `push_int`, `push_str`, `push_bool`, `push_null` - Push values
- `store`, `load` - Local variable access
- `export_var`, `import_var` - Module-level variables

### Arithmetic
- `add`, `sub`, `mul`, `div`, `mod`

### Comparison
- `eq`, `ne`, `lt`, `gt`, `le`, `ge`

### Control Flow
- `jmp`, `jmp_if`, `jmp_if_not` - Jumps
- `call`, `ret` - Function calls

### Capability System
- `cap_call` - Invoke external capabilities

## Formats

CASM has two representations:

### 1. JSON / MessagePack (this crate)

Programs are serialized as JSON or MessagePack for portability — the `casm::Program`
struct serializes to/from this format. Used by `crushc`'s default `--emit vm` output
(in a `.cvm1` binary wrapper) and by `casm::Program::serialize` / `deserialize`.

```json
{
  "functions": {
    "main": {
      "params": [],
      "body": [
        {"op": "push_str", "args": {"value": "Hello"}},
        {"op": "cap_call", "args": {"name": "io.print", "argc": 1}}
      ]
    }
  }
}
```

### 2. Text Assembly (crushc --emit casm / crush-run)

`crushc --emit casm` produces a human-readable text assembly format with mnemonics,
which `crush-run run` accepts as input (via its built-in assembler). This is the same
instruction set, transcribed to a line-oriented text form:

```asm
.func main
    PUSH_STR "Hello"
    CAP_CALL "io.print" 1
    RET
```

Both formats represent the same CASM program; the JSON form is the canonical
serialization for storage/transmission, while the text form is convenient for
ad-hoc inspection and debugging.

## Dependencies

- `crush-errors` - Error types

## Used By

- `crush-frontend` - Compiler output format
- `crush-vm` - Execution input

## 📚 Documentation
- [CASM Specification](https://github.com/nixpt/crush-language-guide) - Assembly format reference in the Crush Language Guide.
