# VM Pipeline Gap Analysis

Last updated: 2026-06-17

## Overview

This document catalogues every known gap, bug, dead code path, and missing feature
in the Crush AST VM pipeline: CASM (text IR) → CVM1 (binary bytecode) → Interpreter.
All file paths are relative to `crates/`.

---

## 1. Pipeline Translation Gaps — Compiler Emits, Translator Bails

The compiler (`crush-frontend/src/compiler.rs`) emits these instruction strings that
`casm_to_vm` (`crush-lang-sdk/src/compile.rs`) has **no handler** for. They hit the
catch-all `other => anyhow::bail!(...)` at compile.rs:200.

### 1.1 DOM opcodes (3) — ✅ Mapped to NOP
| Instruction | Compiler location |
|-------------|-------------------|
| `dom_mutate` | compiler.rs:773 |
| `dom_event_listener` | compiler.rs:791 |
| `dom_query` | compiler.rs:1243 |

### 1.2 AI opcodes (10) — ✅ Mapped to NOP
| Instruction | Compiler location |
|-------------|-------------------|
| `ai_goal_decl` | compiler.rs:823 |
| `ai_progress_update` | compiler.rs:841 |
| `ai_knowledge_share` | compiler.rs:858 |
| `ai_capability_discovery` | compiler.rs:874 |
| `ai_adaptation_request` | compiler.rs:889 |
| `ai_query` | compiler.rs:1280 |
| `ai_tool_chain` | compiler.rs:1352 |
| `ai_agent_delegation` | compiler.rs:1393 |
| `ai_learning_loop` | compiler.rs:1445 |
| `ai_context_aware` | compiler.rs:1463 |

### 1.3 Other (1) — ✅ Mapped to NEW_OBJ
| Instruction | Notes |
|-------------|-------|
| `new_struct` | `OpCode::NewStruct` → NEW_OBJ (loses struct name) |

---

## 2. Silent No-Ops — Compiler Emits, Translator Silently Drops

These instructions are mapped to `NOP` in `casm_to_vm`. The compiler generated code
for them, but it has **zero effect at runtime** with no warning to the user.

| Instruction | compile.rs line | Priority | Why it matters |
|-------------|-----------------|----------|----------------|
| `spawn` | 176 | **Medium** | `async_test.crush` silently does nothing |
| `yield` | 177 | **Medium** | Cooperative multitasking broken |
| `await` | 178 | **Medium** | Async/await completely non-functional |
| `export_var` | 168 | **Low** | `Statement::Export` has no effect |

---

## 3. CASM OpCodes with No CVM1 Bytecode

These `OpCode` variants exist in `casm/src/lib.rs` but have NO equivalent in
`crush-vm/src/bytecode.rs` and NO interpreter support.

| Variant | CASM line | Priority | Notes |
|---------|-----------|----------|-------|
| `ImportVar(String)` | lib.rs:82 | Low | Never emitted by compiler |
| `Rot` | lib.rs:115 | ✅ | Added 0x09, both VMs |
| `Pick(usize)` | lib.rs:116 | ✅ | Added 0x0A, both VMs |
| `Roll(usize)` | lib.rs:117 | ✅ | Added 0x0B, both VMs |
| `Break` | lib.rs:125 | Low | Compiler emits JMP instead |
| `Continue` | lib.rs:126 | Low | Compiler emits JMP instead |
| `TypeOf` | lib.rs:148 | ✅ | Added 0x16, both VMs |
| `Cast(String)` | lib.rs:149 | ✅ | Added 0x17, both VMs |
| `Spawn` | lib.rs:127 | **Medium** | → NOP in translation |
| `Yield` | lib.rs:128 | **Medium** | → NOP in translation |
| `Await { handle }` | lib.rs:129 | **Medium** | → NOP in translation |
| `NewStruct(String)` | lib.rs:143 | Low | → bail in translation |
| `TypeOf` | lib.rs:148 | Low | Type introspection — never emitted |
| `Cast(String)` | lib.rs:149 | Low | Type casting — never emitted |
| `CallHost { ... }` | lib.rs:156 | Low | Capsule system — never emitted |
| `CallInterface { ... }` | lib.rs:164 | Low | Interface system — never emitted |
| `ExportVar(String)` | lib.rs:81 | Low | → NOP in translation |

---

## 4. Portable VM Missing Implementations vs Canonical VM

The `portable_vm.rs` interpreter is missing these opcodes compared to `vm.rs`:

| Opcode | Hex | portable_vm.rs | Impact |
|--------|-----|----------------|--------|
| `EXEC_LANG` | 0x70 | **MISSING** — hits UnknownOpcode | Polyglot execution broken in portable VM |

---

## 5. Interpreter Bugs / Semantic Divergence

### 5.1 MOD remainder sign differs between VMs
- **vm.rs** (line 321): `ai - bi * trunc_div(ai, bi)` — remainder matches truncation toward zero
- **portable_vm.rs** (line 248): `to_i64(&a) % to_i64(&b)` — Rust `%` gives remainder with sign of dividend
- **Impact**: `(-7) MOD 3` produces `-1` in portable VM vs `2` in canonical VM

### 5.2 Dead code in vm.rs
- **Line 326**: `NE => Value::Bool(af != bf)` inside the `ADD|SUB|MUL|DIV|MOD|LT|GT|LE|GE` arm is unreachable.
  `NE` is already caught at the outer level (lines 285-287).

---

## 6. Missing Test Coverage

### 6.1 Opcodes with zero test coverage
| Opcode | tests.rs |
|--------|----------|
| `BITAND`, `BITOR`, `BITXOR`, `BITNOT`, `SHL`, `SHR` | No tests |
| `STR_CONTAINS`, `STR_SPLIT`, `STR_REPLACE`, `STR_JOIN` | No tests |
| `MAKE_RANGE` | No tests |
| `EXEC_LANG` | No tests |
| `SWAP` | Only one basic test |
| `ARR_SET` negative index | No tests |
| `ARR_POP` empty array | No tests |

### 6.2 Error paths with zero test coverage
| Error | vm.rs line | Tested? |
|-------|-----------|---------|
| `StackUnderflow` | 29 | No |
| `StackQuota` | 32 | No |
| `OutputQuota` | 37 | No |
| `CallDepthQuota` | 39 | No |
| `ArithmeticOverflow` | 61 | No |
| `DivByZero` (integer & float) | 60 | No |
| `BadJump` | 49 | No |
| `ConstOutOfRange` | 45 | No |
| `UninitSlot` | 47 | No |
| `BadIndex` | 57 | No |
| `ArrayBounds` | 55 | No |
| `TypeError` (arithmetic) | 53 | No |
| `TypeError` (SET/GET_FIELD) | 53 | No |
| `CapArity` | 67 | No |
| `CapDenied` | 63 | No |
| `CapNotDeclared` | 61 | No |
| `UnknownFunction` | 51 | No |
| `UnknownOpcode` | 41 | No |
| `TruncatedInstruction` | 43 | No |

### 6.3 Capabilities with zero test coverage
| Capability | caps.rs line |
|------------|-------------|
| `str.contains` | 37 |
| `str.split` | 38 |
| `str.replace` | 39 |
| `str.join` | 40 |
| `make_range` | 41 |

---

## 7. Security Issues

### 7.1 exec_lang sandboxing (HIGH)
- **File**: `vm.rs` lines 488–529
- **No executable allowlist**: `Command::new(lang)` runs any binary on PATH
- **No subprocess timeout**: Subprocess can hang VM indefinitely
- **Variable data leaked via env**: `cmd.env(name, val.as_text())` — values visible in `/proc/.../environ`
- **No resource limits**: CPU/memory/fd limits absent
- **Error message leaks**: Stderr content returned directly in error messages
- **Same gaps in portable VM**: portable VM misses EXEC_LANG entirely, but same issues when added

### 7.2 Other
- **`call` function/capability ambiguity** (compile.rs:118–123): If a function name matches a
  capability name with no local function, it's treated as capability call (privilege escalation risk).
- **Output quota bypass** (vm.rs:518–521): subprocess runs to completion before quota check

---

## 8. Example Programs That Would Fail

| Program | Issue |
|---------|-------|
| `examples/crush/greeting.crush` | Uses `sys.*`, `math.*`, `time.*`, `str` caps — not registered |
| `examples/crush/async_test.crush` | `await` → NOP, `async.sleep` not registered |
| `examples/crush/arrays_and_loops.crush` | String indexing `s[0]` → TypeError (ARR_GET on string) |

---

## 9. CVM1 Bytecode Encoding Map (Current)

```
0x00  NOP
0x01  PUSH          (i64)
0x02  PUSH_STR      (u16 const idx)
0x03  POP
0x04  DUP
0x05  SWAP
0x06  PUSH_F64      (f64)
0x07  PUSH_NULL
0x08  PUSH_BOOL     (i64)
0x09-0x0F           FREE (7 slots)
0x10  ADD
0x11  SUB
0x12  MUL
0x13  DIV
0x14  MOD
0x15  NEG
0x16-0x1F           FREE (10 slots)
0x20  EQ
0x21  LT
0x22  GT
0x23  NOT
0x24  NE
0x25  LE
0x26  GE
0x27  AND
0x28  OR
0x29  BITAND
0x2A  BITOR
0x2B  BITXOR
0x2C  BITNOT
0x2D  SHL
0x2E  SHR
0x2F                 FREE (1 slot)
0x30  LOAD          (u16 slot)
0x31  STORE         (u16 slot)
0x32-0x3F           FREE (14 slots)
0x40  JMP           (u32 addr)
0x41  JZ            (u32 addr)
0x42  JNZ           (u32 addr)
0x43-0x4F           FREE (13 slots)
0x50  PRINT
0x51  CAP_CALL      (u16 idx + u8 argc)
0x52  CALL          (u16 func idx)
0x53  RET
0x54  ENTER_TRY     (u32 addr)
0x55  EXIT_TRY
0x56  THROW
0x57  STR_CONTAINS
0x58  STR_SPLIT
0x59  STR_REPLACE
0x5A  STR_JOIN
0x5B  MAKE_RANGE
0x5C-0x5F           FREE (4 slots)
0x60  NEW_ARRAY     (u16 count)
0x61  ARR_GET
0x62  ARR_SET
0x63  ARR_LEN
0x64  ARR_PUSH
0x65  ARR_POP
0x66-0x6F           FREE (10 slots)
0x70  EXEC_LANG     (u16 const idx)
0x71  NEW_OBJ
0x72  SET_FIELD     (u16 const idx)
0x73  GET_FIELD     (u16 const idx)
0x74-0xFE           FREE (139 slots)
0xFF  HALT
```

Total: 60 opcodes assigned, ~195 free slots remaining.

---

## 10. Future Opportunities (Ranked)

1. **[HIGH] Fix MOD remainder sign in portable_vm** — match canonical VM behavior
2. **[HIGH] Remove dead NE arm in vm.rs line 326** — unreachable code
3. **[MEDIUM] Add EXEC_LANG to portable VM** — currently missing
4. **[MEDIUM] Wire ecasm.rs into pipeline** — encrypted CASM exists but unused
5. **[LOW] Add test coverage for error paths** — 18 error paths untested
6. **[LOW] Add test coverage for new opcodes** — 8 opcodes untested
7. **[LOW] Add test coverage for capabilities** — 5 caps untested
8. **[LOW] exec_lang sandboxing** — security hardening
