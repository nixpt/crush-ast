# CRUSH-115 — Add `io.read`: interactive stdin input capability

| Field | Value |
|-------|-------|
| **ID** | CRUSH-115 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |

## Problem

Crush has **zero interactive input capability**. `caps.rs`'s portable registry
only ever registers `io.print` — confirmed by grep, nothing named `io.read`,
`stdin`, or `input` exists anywhere in `crush-vm`/`crush-lang-sdk`. This is
the direct, repeatedly-hit reason every program in `examples/crush/` and the
whole `awesome-crush` collection is self-playing/simulated rather than
actually interactive: pong, tic-tac-toe, lights out, breakout, the 15-puzzle,
and both the Forth and Brainfuck interpreters all had to invent their own
"play against yourself" framing because there was no way to read a real
move, keystroke, or program from a user.

## Impact

An entire category of real programs is impossible today: interactive REPLs
(the natural next step for `forth.crush`/`brainfuck.crush` — feed them a
program from stdin instead of a hardcoded demo string), games with a real
player, and any CLI tool that prompts for input.

## Approach

- Add `io.read` to `crates/crush-vm/src/caps.rs`'s registry, next to
  `io.print`: `argc: Some(0)`, `returns: true`, `privileged: false` — an
  opt-in capability like any other, gated by `--cap io.read` the same way
  `io.print` is gated by `--cap io.print`, not a special sandbox tier.
- Reads one line from stdin, trimming the trailing newline, returns it as a
  Crush string. Decide and document the EOF convention explicitly (empty
  string recommended, matching common scripting-language behavior) rather
  than leaving it implicit.
- Implement identically in **every** backend that implements `io.print`
  today: `portable_vm.rs` (~line 1212), `scheduler.rs` (~line 1408), and the
  AOT C codegen. Follow `io_print.rs`'s own precedent — it exists
  specifically so "every runtime... reproduce this behavior so a program's
  stdout is identical across backends" — and add an analogous `io_read.rs`
  as the single source of truth for line-reading + EOF handling. This is
  directly motivated by CRUSH-114's lesson: `len()` diverges between the VM
  and AOT backends today because there was no shared implementation to
  reference; don't repeat that for `io.read`.
- `crush-jit`'s Cranelift tier is Phase-1-ops-only and already falls back to
  CVM1 on unsupported opcodes (see `crush-notebook`'s
  `eval_jit_source`/`eval_crush_source` fallback for the exact pattern) — a
  blocking stdin read is a reasonable case to explicitly exclude from JIT
  compilation and force the CVM1 fallback; document this rather than letting
  it silently misbehave.

## Definition of done

- [ ] `io.read` registered in `caps.rs`
- [ ] Implemented consistently in `portable_vm`, `scheduler`, and the AOT
      backend (or a documented, deliberate fallback for backends that can't
      support blocking I/O, e.g. JIT)
- [ ] EOF behavior decided and documented
- [ ] `@covers` test through the real pipeline (parse → compile → execute),
      feeding piped stdin, not a smoke test
- [ ] CRUSH-118 (a real interactive demo) can build on this

## Files to modify

- `crates/crush-vm/src/caps.rs` — registry entry
- `crates/crush-vm/src/portable_vm.rs`, `crates/crush-vm/src/scheduler.rs` — cap handlers
- `crates/crush-vm/src/io_read.rs` (new) — shared line-read + EOF logic, mirroring `io_print.rs`
- AOT C backend codegen (wherever `io.print` codegen lives there)

## Gates

None.
