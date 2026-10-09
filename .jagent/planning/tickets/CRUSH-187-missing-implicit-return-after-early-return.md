# CRUSH-187 — Function with an early `return` and a reachable end gets no implicit return (`truncated instruction`)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-187 |
| **Priority** | P0 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | XS |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build); GitHub #37, #92 (pong) |

## Problem

`ensure_return` appends `push_null; ret` only when the body contains **no** `ret`
at all. A function that returns early from a branch but can also fall off the end
therefore has no terminator, and execution runs into the next function or off the
end of the program.

This is the root cause of GitHub #37 and of the `pong.crush` stack underflow in
#92 (awesome-crush `games/pong.crush`'s `pong_tick` has this shape; adding
`return` at its end makes it run to `draw 0-0`). Every polyglot walker hits it
too (Python `def f(n): if n<2: return n` + trailing statements; Rust
`fn f(n:i64)->i64{ if n<2 {return n;} n }`).

## Reproduction

```crush
fn f(c) {
  if c { return }
  print("after")
}
f(false)
print("ok")
```

`crush-run run r.crush` → `[runtime] truncated instruction at 42` (sometimes
`call depth quota exceeded (256)` — layout varies, see CRUSH-188). Expected
`after`, `ok`. Even `fn f(c){ if c { return } }` fails.

Per engine (`fn early(x) { if x { return 1 } }  print(early(false))`): interp
`call depth quota exceeded (256)`; FastVM hangs; JIT `flag=1` or a `JIT stack overflow`
panic; AOT prints `null` (correct). awesome-crush `games/pong.crush`'s `pong_tick` CASM
ends `CALL pong_tick; POP` with no `RET` (with `--max-call-depth 100000` you get #92's
original `stack underflow`). The in-repo `examples/crush/pong.crush` is a different
variant and finishes with a larger `--max-steps`.

## Where

`crates/crush-frontend/src/compiler.rs:~347-352` (`ensure_return`:
`!instrs.iter().any(|i| i.op == "ret")`).

## Success criteria

- [ ] Append the implicit return whenever the last instruction is not an
      unconditional `ret`/`halt`/`jmp` (or always append; dead code is harmless).
- [ ] Regression tests: the repro above, #37's repro, and pong's `pong_tick` shape.
- [ ] Close GitHub #37; #92's pong half.
