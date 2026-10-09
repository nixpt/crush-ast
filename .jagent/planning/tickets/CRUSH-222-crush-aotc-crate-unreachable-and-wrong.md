# CRUSH-222 — The `crush-aotc` crate is unreachable and wrong on most programs

| Field | Value |
|-------|-------|
| **ID** | CRUSH-222 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-144 (superseded if the crate is removed) |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 AOT test drive, reproduced on `main` `5755262` (debug build) |

## Problem

There are two CASM → C backends. `crush-aotc run --backend gcc|clang` (the binary in
`crates/crush-aot`) uses `crates/crush-aot/src/codegen_c.rs`. The **crate**
`crates/crush-aotc` (`AotcCompiler`, NaN-boxed `CrushValue`) has no caller: no binary,
no other crate depends on it, and only its own 25 tests run it. CRUSH-144 asked to put
it in the differential harness; nothing uses it, so first decide whether it should
exist.

Driven over `examples/crush` + 32 one-feature probes through `AotcCompiler` + `cc`, it
matches `crush-run` on 19 of the 52 programs that run cleanly there. Wrong results,
not just unsupported features:

| Program | interp | crush-aotc |
|---|---|---|
| `exception_test` (`try { throw "Oops" } catch e {…}`) | `Caught exception: Oops` | catch block skipped, continues as if nothing was thrown |
| `print(len("hello"))` | `5` | `hello` |
| `print(len([10, 20, 30]))` | `3` | `30` |
| `let a = [10, 20, 30]; print(a[1])` | `20` | `1` |
| `print([1, 2, 3])` | `[1, 2, 3]` | `3` |
| `print("x=" + 2.5)`, `"b=" + true`, `"a=" + [1,2]` | `x=2.5` … | `type error: operands must be numeric` |
| `"abc" < "abd"` | `true` | type error (CRUSH-144) |
| `fn double(n) {…}` (`function_call.crush`) | runs | C compile error: `static CrushValue double(…)` |

15 of the 27 runnable examples fail on the string-concat type error alone.

## Success criteria

- [ ] Decide: delete the crate (its `@kernel`/SIMD pathway would move to a ticket of its
      own), or wire it to a CLI and bring it to parity. Recommendation: delete — the
      `crush-aot` C backend is the one users reach, and two C backends doubles every
      CRUSH-114-style fix.
- [ ] If kept: in the differential harness (CRUSH-144), the rows above fixed, C
      identifiers mangled (`double`, `int`, `char`, …).
