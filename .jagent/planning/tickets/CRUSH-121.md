# CRUSH-121: a kitchen-shaped garbage collector for `crush-vm` (and fix the root set first)

**Status**: open — captured 2026-09-24 (captain), from the [main] foreman's
nanovm review
**Priority**: high for step 1 (a suspected soundness bug); medium for the rest
**Home**: `crates/crush-vm/src/memory.rs` + `fastvm` — the absorbing home for
exosphere's `core/vm/nanovm` memory/GC (extraction playbook W12, atlas §8a).
**Not** in exosphere's in-tree copy: exosphere is being frozen and archived (W18).
Consumers: osmosis (via its `crush-vm` path dep), on Linux today and as a
native Antarikshya capsule later.

## What exists (read 2026-09-24)

- `memory.rs`: a slot `Arena` with a free list, `mark`/`trace(roots)` (walks
  `Array`/`Map`/`Object`/`Tagged`/`Result` refs) and `sweep()`.
- `fastvm/mod.rs`: `collect_garbage()` runs from the run loop every 64
  instructions when `ai_optimizer.should_gc(inputs)` says so (an ML
  prediction model with a heuristic fallback; `alloc_rate` is hard-coded
  `0.0`).
- **Roots = the current `stack` + `locals` only.** Other call frames, globals,
  closure captures, host-held handles (`InterfaceHandle`s, capability state)
  and values parked across a yield are not roots → a sweep can free a live
  object. *Suspected, not yet reproduced.*
- **No test exercises the GC** (the crate's only test file is
  `crate_type_invariant.rs`).
- The module doc over-claims ("cycle detection", "memory safety without
  garbage collection overhead", "O(1) allocation/deallocation").

## "Kitchen-shaped" (the fleet's put-everything-back rule, applied to memory)

Same shape as Antarikshya's kernel reclaim (every reaped task reports
`returned N/N frames, leak 0`) and the `kitchen` worktree tool
(`kitchen clean`, `kitchen stale`):

1. **Kitchens = ownership scopes.** Each capsule / task / call region owns an
   arena; when the scope ends, everything in it is reclaimed wholesale.
2. **Precise tracing inside a long-lived scope.** The root set is every frame,
   globals, captures, host handles and yielded values — never "the current
   frame".
3. **Do the dishes, with an audit.** Every reclaim reports owned vs returned
   (`leak 0`); a `stale` check lists objects nothing gave back. Leaks and
   premature frees become visible instead of silent.
4. **When to collect** from real signals (allocation rate, scope pressure,
   high-water mark); the predictive model stays an optional input, not the
   only gate.

## Steps

- [ ] **1. Reproduce the root-set hole with a failing test**: an object
  reachable only from an outer call frame (or a global / captured value)
  survives `collect_garbage()`. Then fix the root set (all frames + globals +
  captures + host handles + yield state).
- [ ] 2. GC tests: cycles collected; live graphs survive; stats (`peak_usage`,
  freed counts) match; a stress test under `should_gc` always-true.
- [ ] 3. Scoped arenas + wholesale reclaim at scope end, with the owned/returned
  audit line.
- [ ] 4. A `stale` report (objects never returned) for debug builds.
- [ ] 5. Real trigger signals (compute `alloc_rate`); keep the model optional.
- [ ] 6. Trim the module doc to what the code does.

## Related

- nanovm's `memory/arena.rs` is a near-verbatim twin of `memory.rs`; its
  `ai_optimizer` twin (`VmOptimizer`, "predictive GC") is crush-vm's
  `ai_optimizer`. Both retire with exosphere once this lands.
- CRUSH-122 (stdlib convergence) — the other crush-ast gap from the same review.
