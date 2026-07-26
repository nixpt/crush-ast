# CRUSH-69 — JS `Math.sin` / `Math.cos` / `Math.tan` silently miscompile

| Field | Value |
|-------|-------|
| **ID** | CRUSH-69 |
| **Priority** | P1 — same silent-wrong-answer class as CRUSH-65 |
| **Status** | Done (PR pending) |
| **Phase** | Correctness / JS walker |
| **Assignee** | nixp |
| **Dependencies** | CRUSH-65 ✅ (Math.* case mapping + CapabilityCall path) |
| **Estimated effort** | S |

## Problem

CRUSH-65 mapped eight `Math.*` names onto `math.*` host caps. `Math.sin` /
`Math.cos` / `Math.tan` were left out of the producer list even though the
consumers already exist (`MathSinCap` / `MathCosCap` / `MathTanCap` in
`crush-lang-sdk` stdlib). They still fall through to the dotted method-call
path (`load Math` + `cap_call sin`) and yield a wrong number with no error.

Flagged in CRUSH-65 Findings §4 / TASKS.md Done-session gap.

## Success criteria

- [x] `Math.sin` / `Math.cos` / `Math.tan` lower to `CapabilityCall { "math.<op>" }`
- [x] End-to-end numeric tests (same shape as `math_builtins_test.rs`)
- [x] Existing CRUSH-65 math tests stay green
- [x] CAST dump pin includes the three new names

## Non-goals

- `Math.random` (still no float-in-[0,1) cap)
- AOT opcode-vs-cap_call structural NULL fallthrough (CRUSH-65 Findings §1)
- Shared builtin registry refactor
- Float Display `"50.0"` vs JS `"50"`

## Resolution

Shipped on `agent/nixp/CRUSH-69`: extended the CRUSH-65 `CapabilityCall` arm in
`lower_swc.rs` with `Math.sin` / `Math.cos` / `Math.tan` (via existing
`math_builtin()`), plus four e2e numeric tests and CAST-name pins.