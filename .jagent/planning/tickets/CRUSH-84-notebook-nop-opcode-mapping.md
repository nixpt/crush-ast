# CRUSH-84 — crush-notebook: casm_to_assembly silently maps unknown opcodes to NOP

| Field | Value |
|-------|-------|
| **ID** | CRUSH-84 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | Correctness spine (s412) — CROSS-REPO |
| **Repo** | `crush-workspace/crush-notebook` (ticket anchored here; work dispatches there) |

## Problem

Panini client-survey capture (2026-08-02): the notebook kernel's
`casm_to_assembly` (`kernel/src/main.rs:403-478` in
`/home/nixp/WORKSPACE/projects/crush-workspace/crush-notebook`; re-verify)
maps any casm opcode it doesn't recognize to NOP and continues — users get
wrong programs instead of errors. Same silent-miscompile bug class as the JIT
TAG_NULL catch-all (CRUSH-72): a translator that fabricates semantics for
inputs it doesn't understand. Every new crush-ast opcode (the M5 additions,
e.g.) silently degrades in the notebook until someone notices.

## Approach

Hard-error arm: unknown opcode → kernel error surfaced to the cell with the
opcode name + a version-skew hint (notebook built against older casm). Add a
test feeding a fabricated unknown opcode. Consider generating the mapping from
casm's opcode enum so skew is caught at compile time.

## Definition of done

- [ ] Unknown opcode → loud error with opcode named (test)
- [ ] Notebook test suite green
- [ ] Version-skew note in the error text or docs

## Files in scope

- `crush-notebook/kernel/src/main.rs` (in crush-workspace repo — branch there,
  not in crush-ast)

## Gates

None.


## Dispatch

Imported from `workspace-meta/prompts/crush-backlog/CRUSH-84.txt` on 2026-08-24 so this tracked ticket contains the dispatch prompt metadata.

- Branch: `agent/horse/CRUSH-84`
- Repo: `crush-workspace`
- Turns: `40`
- Runner: `claude`
- Scope: this ticket file is the canonical implementation spec; read it before changing code.
- Discipline: verify the repro against current `main` first, commit and push each meaningful unit, and update this ticket with the result.
- Verification: satisfy this ticket's Definition of done, include test evidence, and quote the real post-commit `HEAD` hash.
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs` unless this ticket explicitly scopes them; flag `crush_cast::Function`/`Program` shape changes before landing.
- Halt: stop and DM foreman if gates are unmet, scope is wrong, the repro no longer exists, sandbox blocks required work, or budget is nearly exhausted.
