# CRUSH-232 — `casm_to_vm` lowering completeness over the CASM `OpCode` set

| Field | Value |
|-------|-------|
| **ID** | CRUSH-232 |
| **Priority** | P2 |
| **Status** | Done (pending review) |
| **Phase** | M1 |
| **Assignee** | panini |
| **Dependencies** | none |
| **Filed by** | foreman — NanoVM → Osmosis (CVM1) retirement, producer side |

## Problem

`crush_lang_sdk::compile::casm_to_vm` bailed `Unsupported CVM1 opcode` for 38 of the 110 CASM
`OpCode` variants, and emitted a `SPAWN` that failed assembly for a 39th. Several (`str_trim`, `math_pow`,
`new_tuple`…) are emitted by the frontend, so programs using them could not run on CVM1 at all.
The rough grep in the ticket prompt was close but not exact (e.g. `arr_set`/`index`/`new_array`
were already lowered; `dom_*` and `ai_capability_discovery` "lowered" but only to `NOP`).

## Result (measured by `tests/casm_lowering_completeness.rs`)

| | Before | After |
|---|---|---|
| Variants in `casm::OpCode` | 110 | 110 |
| Lower to a real CVM1 op / cap call | 64 | 98 |
| Lower to `NOP` (stub, listed in `NOP_STUBS`) | 7 | 7 |
| Unsupported (`UNSUPPORTED` allowlist) | 38 | 5 |
| Lower but fail assembly (`spawn`) | 1 | 0 |

(Raw test output: before = 71 lowered / 38 unsupported / 1 assembly failure (`spawn`); after =
105 lowered / 5 unsupported. The raw "lowered" counts include the 7 NOP stubs: 71-7 = 64, 105-7 = 98.)

Newly lowered (each has a run-through-CVM1 value test, on the scheduler **and** `PortableVm`):

- `bit_and bit_or bit_xor bit_not shl shr`
- `rot` (as `ROLL 2`, see below) `pick n` `roll n`
- `type_of` `cast`
- `math_pow math_sqrt math_abs math_round math_floor math_ceil`
- `str_starts_with str_ends_with str_to_upper str_to_lower str_trim`
- `arr_len arr_push arr_pop`
- `new_tuple new_list new_vector new_set` and `tuple_push list_push vector_push set_push`
- `spawn` fixed: emitted bare `SPAWN`; CVM1 needs the argc operand the frontend already provides.

### Decisions

- **`rot` → `ROLL 2`, not `ROT`.** CVM1's `ROT` (scheduler + PortableVm) is `[x,y,z] → [y,x,z]`;
  FastVM/JIT `Rot` is `[x,y,z] → [y,z,x]`. `ROLL 2` is the latter. The CVM1 `ROT` bug is captured
  separately; crush-vm was not touched.
- **`new_*` `size` is a capacity hint**, matching the existing `new_array` lowering and what the
  frontend emits (`new_X size` followed by one `X_push` per element). All collections lower to
  `NEW_X 0`.

### Explicit `Unsupported` allowlist

| Op | Why CVM1 cannot express it |
|----|----------------------------|
| `break`, `continue` | The instruction carries no loop target; the frontend lowers loop exits to `jmp`. CVM1 has no loop stack. |
| `import_var` | Reads a variable exported from another execution context; CVM1 has one flat frame memory and no import channel. |
| `call_host` | Calls a capsule function by 32-byte IC id; CVM1 has only name-based `CAP_CALL`. |
| `call_interface` | CSCS v1 call through an ObjectHandle/token; CVM1 has no handle table or interface dispatch. |

Not on the list although the prompt grouped them there: `exec_lang`, `spawn`, `await` already
lowered to real CVM1 opcodes (and `spawn` is now correct); `export_var` lowers to `NOP`.

### Lowered to `NOP` only (not real lowerings)

`export_var`, `dom_query`, `dom_mutate`, `dom_event_listener`, `ai_adaptation_request`,
`ai_capability_discovery`, `ai_semantic_switch`. Pre-existing; now visible and pinned by
`NOP_STUBS`. Whether each should get a real lowering or become `Unsupported` is a follow-up
(captured in TASKS).

## Success criteria

- [x] Test enumerates every `OpCode` variant; a variant that is neither lowered nor allowlisted fails it.
  Shown failing once by removing the `bit_and` arm (`ops neither lowered nor on the UNSUPPORTED
  allowlist: ["bit_and"]`), then restored.
- [x] Before/after table (above).
- [x] Each newly lowered op has a value-asserting CVM1 run test.
- [x] `cargo test -p crush-lang-sdk` and `-p crush-vm` pass.
