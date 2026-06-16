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

## Format

Programs are serialized as JSON or MessagePack for portability:

```json
{
  "functions": [
    {
      "name": "main",
      "instructions": [
        {"op": "push_str", "value": "Hello"},
        {"op": "cap_call", "name": "io.print"}
      ]
    }
  ]
}
```

## Dependencies

- `crush-errors` - Error types

## Used By

- `crush-frontend` - Compiler output format
- `crush-vm` - Execution input

## 📚 Documentation
- [CASM Specification](https://github.com/nixpt/crush-language-guide) - Assembly format reference in the Crush Language Guide.
