# CRUSH-101 — Notebook self-hosting walker pipeline (in-kernel, no subprocess)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-101 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M11 — CROSS-REPO (crush-workspace/crush-notebook) |

## Problem

M11's notebook goal: cells in any of 12+ supported languages execute through
the walker→AOT (or walker→VM) path INSIDE the kernel — no subprocess — sharing
one arena + variable scope across cells of different languages. Verify the
kernel's current exec path first (read-only in crush-workspace/crush-notebook;
also mind CRUSH-84's NOP bug — fix that first or alongside).

## Approach

Kernel links crush-frontend + crush-vm directly (it already consumes crush-ast
crates); per-cell: walker-lower → compile into the SESSION program (append
functions, shared globals) → execute on the session VM. Cross-language
variable sharing rides the shared value model (Value types are
language-agnostic post-lowering). CRUSH-83's compile cache makes the
cell-edit loop fast — cross-reference. Subprocess path stays as fallback for
languages without in-process walkers (stated per-language table).

## Definition of done

- [ ] Python cell defines x; JS cell reads x; crush cell calls a Python-defined
      fn — all in-process, one VM session
- [ ] Per-language table: in-process vs subprocess-fallback (no silent gaps)
- [ ] CRUSH-84 hard-error behavior preserved

## Files in scope

- crush-notebook kernel (branch in crush-workspace); crush-ast only for API gaps

## Gates

CRUSH-98/103 for AOT-tier; VM-tier can start after CRUSH-84.
