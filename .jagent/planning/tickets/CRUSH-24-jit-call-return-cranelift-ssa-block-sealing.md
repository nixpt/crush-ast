# CRUSH-24 — JIT `CALL`/`RETURN` dispatch cascade violates Cranelift's SSA block-sealing invariant

| Field | Value |
|-------|-------|
| **ID** | CRUSH-24 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M2 (JIT backend) |
| **Assignee** | unassigned |
| **Dependencies** | none (blocks `agent/buffy/CRUSHAST-CRUSH-1` — the whole branch is unmerged pending this) |
| **Estimated effort** | M |

## Problem

`crush-jit`'s Cranelift-based compiler (`crates/crush-jit/src/compiler.rs`,
`build_fn`) panics on every program that contains a function `CALL`/`RETURN`.
5 of the 44 `crush-jit` lib tests fail identically:

```
thread 'tests::test_call_simple_no_args' panicked at
cranelift-frontend-0.133.1/src/ssa.rs:407:9:
assertion failed: !self.is_sealed(block)
```

Same panic for `test_call_with_args`, `test_call_nested`,
`test_call_multiple_functions`, `test_call_chain_with_arithmetic` — every
test that exercises a function call, none that don't. Not a flake; 100%
reproducible.

## Root cause

`build_fn` (lines ~142-210) does a **single forward pass** over `blocks`
and seals each block immediately after emitting its instructions:

```rust
for &(off, ref instrs) in blocks {
    let block = map[&off];
    bld.switch_to_block(block);
    // ... emit instructions ...
    bld.seal_block(block);   // line 189 — sealed right away
}
```

Cranelift's incremental SSA construction (`cranelift-frontend`'s
`seal_block`) requires that **all predecessors of a block are known before
it is sealed** — sealing early is only valid for control flow where nothing
outside the current linear scan ever jumps into an already-processed block.

The `Call` opcode (line ~534, `b.ins().jump(target_block, ...)`) breaks this
assumption: `target_block` is the **callee's entry block**, which is just
another entry in `map` built by the same single-pass loop. A function can be
called from multiple call sites (multiple predecessors into one entry
block), and a call site can appear *after* the callee's own block has
already been visited-and-sealed in program order — at which point emitting
the call's `jump()` tries to add a new predecessor edge to an already-sealed
block, tripping the assertion. `return_blocks` (the per-call-site return
targets, built + sealed in a *separate* pass at lines 195-209, after the
main loop) don't have this problem — only the direct entry-block jump in
the `Call` case does.

## Reproduction

```bash
cd projects/crush-ast   # branch agent/buffy/CRUSHAST-CRUSH-1 (668e556)
cargo test -p crush-jit tests::test_call_simple_no_args -- --nocapture
```

Minimal case (from `test_call_simple_no_args`, `crates/crush-jit/src/lib.rs`):
```
0: CALL "foo" 0    // call foo
1: HALT
2: PUSH 42         // foo entry
3: RETURN
```

## Technical approach (starting points, not a committed design)

1. **Defer sealing of call-target blocks.** Don't seal a regular block at
   line 189 if it is ever the target of a `Call` (i.e., it's a function
   entry point) until every `Call` instruction in the program that targets
   it has been emitted. Requires either a pre-pass to collect
   "which blocks are call targets" before the main loop, or restructuring
   to two passes: (a) emit all instructions/edges without sealing, (b) seal
   every block once all edges are known.
2. **Simplest correct fix**: don't seal *any* block inside the main loop
   (remove line 189 entirely) — seal all blocks (regular + return_blocks)
   in one final pass after `build_fn` has finished emitting every
   instruction and every call/branch edge. This is the standard Cranelift
   pattern for "compile the whole function, then seal everything" and
   avoids needing to classify blocks in advance — verify it doesn't
   regress the already-passing 39 tests (branches/loops currently rely on
   the block being sealed before its own internal `brif`/`jump` targets are
   built — check `emit_return_dispatch`, `int_bb`/`float_bb`/`merge_bb`
   arithmetic-coercion blocks, and the ternary `tb`/`eb` blocks don't
   assume earlier sealing for correctness, only for the SSA builder's
   variable-resolution timing).
3. Whichever approach: add a regression test with **two call sites into the
   same function** (not just `test_call_multiple_functions`, which calls
   two *different* functions) to specifically cover the multi-predecessor
   case that's the actual root cause.

## Files to modify

- `crates/crush-jit/src/compiler.rs` — `build_fn` (block creation/sealing
  order, lines ~142-210), the `Call` case's `target_block` jump (~line 534,
  552), possibly `emit_return_dispatch` (~line 334) if the sealing
  restructure affects it.

## Non-goals

- Fixing anything in the AOT backends (`crush-aot`'s `codegen.rs`/
  `codegen_c.rs`) — those are a separate code path (commit `20ddcaf` on
  this branch, CRUSH-1's AI-opcode stub wiring), unaffected by this bug and
  not covered by the failing tests.
- Redesigning the calling convention or return-dispatch mechanism — the
  `br_table`-via-`brif`-cascade approach in `emit_return_dispatch` is
  out of scope; only the block-sealing order is broken.

## Done condition

All 5 currently-failing tests in `crates/crush-jit/src/lib.rs`
(`test_call_simple_no_args`, `test_call_with_args`, `test_call_nested`,
`test_call_multiple_functions`, `test_call_chain_with_arithmetic`) pass,
plus a new multi-call-site-into-one-function regression test, with the
existing 39 passing tests still green. Only then is
`agent/buffy/CRUSHAST-CRUSH-1` mergeable.

## References

- Branch: `agent/buffy/CRUSHAST-CRUSH-1` (origin, `668e556`) — where the bug
  was found via `foreman-merge-wave` review (s391, 2026-07-20), before
  merge, not after.
- Failing tests added in the same commit that surfaced this ticket
  (`crates/crush-jit/src/lib.rs`, "crush-jit: add CALL/RETURN regression
  tests").
