# CASM Specification v1.0

**Crush Assembly Bytecode - Frozen Specification**

**Version:** 1.0.0  
**Date:** 2026-01-15  
**Status:** FROZEN

---

## Overview

CASM (Crush Assembly) is the low-level bytecode format for the Crush Virtual Machine. The Crush compiler translates CAST (Abstract Syntax Tree) into CASM, which is then executed by the VM.

## Design Principles

1. **Stack-Based**: All operations use a value stack
2. **JSON Serialization**: Human-readable for debugging
3. **Capability-Based**: Resource access through capabilities
4. **Green Threads**: Cooperative multitasking with yield

---

## Document Structure

### Program Format

```json
{
  "version": "1.0",
  "lang": "crush",
  "functions": {
    "main": {
      "params": ["arg1", "arg2"],
      "instructions": [ /* Instruction[] */ ],
      "meta": {}
    }
  },
  "manifest": {
    "permissions": ["io.print", "fs.read"]
  }
}
```

**Fields:**
- `version` (string): CASM version
- `lang` (string, optional): Source language
- `functions` (object): Map of function name to Function
- `manifest` (object, optional): Capability permissions

### Function Format

```json
{
  "params": ["x", "y"],
  "instructions": [
    {"op": "load", "args": {"name": "x"}},
    {"op": "load", "args": {"name": "y"}},
    {"op": "add", "args": {}},
    {"op": "ret", "args": {}}
  ],
  "meta": {"file": "main.crush", "line": 1}
}
```

### Instruction Format

```json
{
  "op": "push_int",
  "args": {"value": 42},
  "meta": {"line": 5, "file": "main.crush"}
}
```

**Fields:**
- `op` (string): Opcode name
- `args` (object): Instruction arguments
- `meta` (object, optional): Source metadata

---

## Instruction Set

### Stack Operations

#### push_int
Push integer onto stack.

```json
{"op": "push_int", "args": {"value": 42}}
```

**Stack:** `[] → [Int(42)]`

---

#### push_str
Push string onto stack.

```json
{"op": "push_str", "args": {"value": "hello"}}
```

**Stack:** `[] → [Str("hello")]`

---

#### push_bool
Push boolean onto stack.

```json
{"op": "push_bool", "args": {"value": true}}
```

**Stack:** `[] → [Bool(true)]`

---

#### push_null
Push null onto stack.

```json
{"op": "push_null", "args": {}}
```

**Stack:** `[] → [Null]`

---

#### pop
Remove top value from stack.

```json
{"op": "pop", "args": {}}
```

**Stack:** `[value] → []`

---

#### dup
Duplicate top stack value.

```json
{"op": "dup", "args": {}}
```

**Stack:** `[value] → [value, value]`

---

### Variable Operations

#### store
Store top stack value in variable.

```json
{"op": "store", "args": {"name": "x"}}
```

**Stack:** `[value] → []`  
**Effect:** `variables[name] = value`

---

#### load
Load variable onto stack.

```json
{"op": "load", "args": {"name": "x"}}
```

**Stack:** `[] → [value]`  
**Effect:** Push `variables[name]`

---

#### export_var
Export variable to module scope.

```json
{"op": "export_var", "args": {"name": "result"}}
```

**Stack:** `[value] → []`  
**Effect:** `exports[name] = value`

---

#### import_var
Import variable from module.

```json
{"op": "import_var", "args": {"name": "helper"}}
```

**Stack:** `[] → [value]`  
**Effect:** Push `imports[name]`

---

### Arithmetic Operations

#### add
Add two numbers.

```json
{"op": "add", "args": {}}
```

**Stack:** `[a, b] → [a + b]`  
**Types:** Int + Int → Int, Float + Float → Float

---

#### sub
Subtract two numbers.

```json
{"op": "sub", "args": {}}
```

**Stack:** `[a, b] → [a - b]`

---

#### mul
Multiply two numbers.

```json
{"op": "mul", "args": {}}
```

**Stack:** `[a, b] → [a * b]`

---

#### div
Divide two numbers.

```json
{"op": "div", "args": {}}
```

**Stack:** `[a, b] → [a / b]`  
**Error:** Division by zero throws exception

---

### Comparison Operations

#### eq
Test equality.

```json
{"op": "eq", "args": {}}
```

**Stack:** `[a, b] → [Bool(a == b)]`

---

#### ne
Test inequality.

```json
{"op": "ne", "args": {}}
```

**Stack:** `[a, b] → [Bool(a != b)]`

---

#### lt
Test less than.

```json
{"op": "lt", "args": {}}
```

**Stack:** `[a, b] → [Bool(a < b)]`

---

#### gt
Test greater than.

```json
{"op": "gt", "args": {}}
```

**Stack:** `[a, b] → [Bool(a > b)]`

---

#### le
Test less than or equal.

```json
{"op": "le", "args": {}}
```

**Stack:** `[a, b] → [Bool(a <= b)]`

---

#### ge
Test greater than or equal.

```json
{"op": "ge", "args": {}}
```

**Stack:** `[a, b] → [Bool(a >= b)]`

---

### Control Flow

#### jmp
Unconditional jump.

```json
{"op": "jmp", "args": {"target": 10}}
```

**Effect:** `pc = target`

---

#### jmp_if
Jump if top of stack is true.

```json
{"op": "jmp_if", "args": {"target": 10}}
```

**Stack:** `[Bool(cond)] → []`  
**Effect:** If `cond`, then `pc = target`

---

#### jmp_if_not
Jump if top of stack is false.

```json
{"op": "jmp_if_not", "args": {"target": 10}}
```

**Stack:** `[Bool(cond)] → []`  
**Effect:** If `!cond`, then `pc = target`

---

#### call
Call function.

```json
{"op": "call", "args": {"function": "add", "argc": 2}}
```

**Stack:** `[arg1, arg2, ...] → [result]`  
**Effect:** Call function with `argc` arguments

---

#### ret
Return from function.

```json
{"op": "ret", "args": {}}
```

**Stack:** `[value] → []`  
**Effect:** Return `value` to caller

---

### Exception Handling

#### enter_try
Enter try block.

```json
{"op": "enter_try", "args": {"handler": 20}}
```

**Effect:** Push exception handler at instruction `handler`

---

#### exit_try
Exit try block.

```json
{"op": "exit_try", "args": {}}
```

**Effect:** Pop exception handler

---

#### throw
Throw exception.

```json
{"op": "throw", "args": {}}
```

**Stack:** `[error_value] → []`  
**Effect:** Throw exception with `error_value`

---

### Capability Calls

#### cap_call
Call capability.

```json
{"op": "cap_call", "args": {"name": "io.print", "argc": 1}}
```

**Stack:** `[arg1, arg2, ...] → [result]`  
**Effect:** Invoke capability with `argc` arguments

---

### Data Structures

#### array_new
Create array from stack values.

```json
{"op": "array_new", "args": {"size": 3}}
```

**Stack:** `[v1, v2, v3] → [Array([v1, v2, v3])]`

---

#### index_get
Get array/map element.

```json
{"op": "index_get", "args": {}}
```

**Stack:** `[array, index] → [value]`  
**Effect:** Push `array[index]`

---

#### index
Array indexing (for loops).

```json
{"op": "index", "args": {}}
```

**Stack:** `[array, index] → [value]`  
**Effect:** Push `array[index]`

---

#### len
Get length of collection.

```json
{"op": "len", "args": {}}
```

**Stack:** `[collection] → [Int(length)]`  
**Types:** Array, Map, String

---

#### make_range
Create range array.

```json
{"op": "make_range", "args": {}}
```

**Stack:** `[start, end] → [Array([start..end])]`  
**Example:** `[0, 5] → [Array([0,1,2,3,4])]`

---

### Concurrency

#### spawn
Spawn green thread.

```json
{"op": "spawn", "args": {"function": "worker", "argc": 1}}
```

**Stack:** `[arg1, ...] → []`  
**Effect:** Create new task running `function`

---

#### yield
Yield control to scheduler.

```json
{"op": "yield", "args": {}}
```

**Effect:** Pause current task, resume later

---

#### await
Await async operation (MVP: no-op).

```json
{"op": "await", "args": {}}
```

**Stack:** `[value] → [value]`  
**Effect:** Currently pass-through, future: poll futures

---

### Object System

#### new_struct
Create struct instance.

```json
{"op": "new_struct", "args": {"name": "Point"}}
```

**Stack:** `[] → [Struct(Point)]`

---

#### get_field
Get object field.

```json
{"op": "get_field", "args": {"name": "x"}}
```

**Stack:** `[object] → [value]`  
**Effect:** Push `object.field`

---

#### set_field
Set object field.

```json
{"op": "set_field", "args": {"name": "x"}}
```

**Stack:** `[object, value] → []`  
**Effect:** `object.field = value`

---

### Polyglot

#### exec_lang
Execute code in language sandbox.

```json
{"op": "exec_lang", "args": {"lang": "python", "code": "print('hi')"}}
```

**Effect:** Execute `code` in WASI sandbox

---

## Runtime Values

### Value Types

- **Int**: 64-bit signed integer
- **Float**: 64-bit floating point
- **String**: UTF-8 string
- **Bool**: true or false
- **Null**: null value
- **Ref**: Reference to heap object

### Heap Objects

- **Array**: Dynamic array of values
- **Map**: Hash map of string keys to values
- **Struct**: Named struct instance
- **Str**: Heap-allocated string

---

## Calling Convention

### Function Call

1. Push arguments onto stack (left to right)
2. Execute `call` instruction
3. Callee pops arguments
4. Callee pushes return value
5. Execute `ret` instruction
6. Caller receives return value on stack

### Example

```json
// add(2, 3)
{"op": "push_int", "args": {"value": 2}},
{"op": "push_int", "args": {"value": 3}},
{"op": "call", "args": {"function": "add", "argc": 2}}
// Stack now has result: 5
```

---

## Exception Model

### Try/Catch

```json
// try { risky() } catch e { handle(e) }
{"op": "enter_try", "args": {"handler": 5}},
{"op": "call", "args": {"function": "risky", "argc": 0}},
{"op": "exit_try", "args": {}},
{"op": "jmp", "args": {"target": 8}},
// Handler at instruction 5:
{"op": "store", "args": {"name": "e"}},
{"op": "call", "args": {"function": "handle", "argc": 1}},
// Continue at instruction 8
```

---

## Manifest Format

### Capability Permissions

```json
{
  "permissions": [
    "io.print",
    "io.read",
    "fs.read",
    "fs.write",
    "net.connect"
  ]
}
```

Programs must declare required capabilities in manifest.

---

## Version History

- **v1.0.0** (2026-01-15): Initial frozen specification
  - 40+ instructions
  - Exception handling
  - Green threads
  - Capability system
  - Polyglot support

---

## Compatibility

CASM v1.0 is the **frozen baseline**. Future versions maintain backward compatibility. New instructions may be added in minor versions. Breaking changes require major version bump.

**Implementations:**
- Crush VM (Rust): Full support
- WASM Runtime: Planned

---

**End of CASM v1.0 Specification**
