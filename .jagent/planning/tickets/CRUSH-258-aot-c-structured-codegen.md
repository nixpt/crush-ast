# CRUSH-258 — AOT C: structured codegen instead of a per-instruction switch over a boxed stack

| Field | Value |
|-------|-------|
| **ID** | CRUSH-258 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M2 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-257 (typed locals make this pay off) |
| **Estimated effort** | L |
| **Filed by** | claude (vega) — 2026-10-12 JS-vs-Node benchmark (CRUSH-261) |

## Problem

`emit_c_function` (~835-880) emits `while (1) { _gc_safepoint(); switch (_pc) { case N: ... } }`: one case per
CASM instruction, each pushing/popping boxed `Value`s, e.g. for `s + i*i`:

```c
case 9:  _push(_locals[__lb + 1]); _pc=10; break;
case 10: _push(_locals[__lb + 1]); _pc=11; break;
case 11: { Value _b = _pop(); Value _a = _pop(); _push(_mul(_a,_b)); } _pc=12; break;
```

It's an interpreter with dispatch compiled in: a safepoint and a switch per instruction, every operand through
memory. The frontend also emits a dead `push_null; pop` after each assignment statement.

## Success criteria

- [ ] Split each function into basic blocks (targets of jumps); emit each block once with `goto`, no `_pc`
      switch except for computed targets if any.
- [ ] Within a block, symbolically execute the stack: straight-line expressions become C expressions/temps
      (`_li_s = (_li_s + _li_i * _li_i) % 1000003;` with CRUSH-257), spilling to the Value stack only at
      block boundaries/calls.
- [ ] `_gc_safepoint()` only at loop back-edges, calls and allocations.
- [ ] Peephole (frontend or backend): drop `push_null; pop` pairs.
- [ ] Parity + GC stress green; CRUSH-261 numbers before/after.
