# CRUSH-34 — `spawn`/`await`/`yield` to VM execution (3 opcodes + scheduling rules)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-34 |
| **Priority** | P2 — depends on M2's steady-state cooperative scheduler (per `.dejavue/decisions.md` 2026-07-19 entry); touches the CVM1 scheduler core; sibling of CRUSH-32/33 |
| **Status** | Superseded |
| **Phase** | M5 |
| **Assignee** | unassigned |
| **Dependencies** | none on `CRUSH-32`/`CRUSH-33` (parallels — same VM-side dispatch shape; can be developed in either order); requires M2 cooperative scheduler stable per `.dejavue/decisions.md` 2026-07-01 "[ADOPTED] Crush native codegen (JIT) architecture design" entry |
| **Estimated effort** | L |

> Superseded s412: canonical file is `CRUSH-34-spawn-await-yield-vm-wire-up.md` (Commit 1 79390d7 landed; Commits 2–3 pending).

## Origin

Filed s394 (2026-07-23) from `.jagent/planning/ROADMAP.md` M5 section.
`TODO.md` priority 1 line 4:
> Wire spawn/await/yield to VM execution

## Problem

`spawn` (concurrent task creation), `await` (task synchronization),
and `yield` (cooperative scheduler output) are not currently
executable ops in any VM tier. The CVM1 scheduler has a cooperative
green-thread model (the phrase appears as an `@module purpose:`
annotation example in `docs/design/ai-native-roadmap.md` line 105;
the cooperative-scheduling semantics themselves are established by
the existing `crush-vm/src/scheduler.rs` reference design prior to
this ticket — there is no adopted `.dejavue/decisions.md` entry
for the green-thread model itself, only the annotation language
that *describes* it). What CRUSH-34 adds is the explicit opcodes
for `spawn`/`await`/`yield` semantics that the scheduler model
currently lacks.

AI-agent programs that need to spawn subtasks (e.g., parallel
branch evaluation, async fetches, multi-agent coordination) have
no path today beyond spawning subprocesses via CRUSH-20's
`buckets`-sandboxed polyglot exec lane — which is for **external**
languages, not internal concurrency.

## Success criteria

- [ ] Three new CASM instruction forms:
      - `Spawn(Box<Expr>) -> Value::TaskRef`
      - `Await(Value::TaskRef) -> Value`
      - `Yield`
- [ ] Dispatched at **all 5 execution tiers** (scheduler, portable_vm,
      fastvm, aot-rust, aot-c). AOT tier requires thread-pool
      integration (`aot-rust`/`aot-c` cannot meaningfully support
      Spawn before M2's JIT phase 5 is complete — flag explicitly in
      the ticket; per CRUSH-32 same scoping).
- [ ] A small `examples/crush/spawn_await_demo.crush` example:
      ```
      let main = async {
        let a = spawn(() => compute_a());
        let b = spawn(() => compute_b());
        await a;
        await b;
        return a + b;
      };
      ```
      runs end-to-end on PortableVM (target tier for v0).
- [ ] **Differential coverage** for these 3 ops (parity bar with
      CRUSH-32/33): all 5 tiers agree on output, with explicit
      exclusion of AOT tiers in v0 (per the thread-pool caveat
      above).
- [ ] The existing scheduler's green-thread model is preserved
      (no regression to the cooperative-thread story told in
      `.dejavue/decisions.md` 2026-07-19).

## Technical approach

1. **Opcodes.** Add 3 new `casm::Instruction` variants.
2. **Dispatcher.** Spawn creates a new `Task` struct (pointer to
   closure, join handle, in-flight state) and emits the
   `TaskRef`. Await suspends the current frame until the TaskRef
   is joined. Yield is a no-op (after the scheduler resumes) that
   returns to the host-cap dispatcher.
3. **Scheduler integration.** Extend `scheduler.rs::run` to support
   a small thread pool — or single-threaded cooperative
   alternation if M2 phase-5 hasn't shipped ExoLight integration
   yet. **The whole ticket is gated on M2's cooperative story
   being stable**, so this decision is made *at ticket start* by
   re-reading `.dejavue/decisions.md` for the relevant phase-5
   status.
4. **AOT tier.** Out of scope for v0 (per CRUSH-32 scoping); a
   follow-up CRUSH-NN ticket covers AOT integration once the
   scheduler pool is stable.

## Files to modify

- `crates/casm/src/lib.rs` — 3 new `Instruction` variants
- `crates/crush-frontend/src/compiler/mod.rs` — emit the 3 ops
- `crates/crush-vm/src/scheduler.rs` — dispatch + pool (or pool-like
  alternation) — this is the load-bearing file; careful review
  required since the existing scheduler is the M1-correctness sweep's
  hot path
- `crates/crush-vm/src/portable_vm.rs` — 3 mirror branches
- `crates/crush-vm/src/fastvm/` — 3 lowering entries
- `examples/crush/spawn_await_demo.crush` (new) — demo

## Non-goals

- **No full actor model.** Spawn/await/yield are deeply orthogonal
  to a message-passing actor system; that's a separate ticket
  (post-M11).
- **No distributed spawn.** All spawned tasks share the same
  process. Cross-process spawn is a different op (post-M12 in the
  M-distributed-runtime band).
- **No cancellation semantics.** This ticket does not add task
  cancellation; that's a `cancel TaskRef` opcode, deferred.
- **No AOT tier support in v0.** Parked — see Technical Approach.

## Cross-references

- `.jagent/planning/ROADMAP.md` — M5 ticket 8 of 8
- `TODO.md` priority 1 line 4
- CRUSH-32 / CRUSH-33 (parallels — VM dispatch shape)
- `.dejavue/decisions.md` 2026-07-19T01:01:18 entry — CRUSH-19 cooperative
  HostCap deadline (related: the *cooperative-timeout* semantics for
  blocking caps, which is the sibling shape this ticket extends with
  cooperative-task scheduling)
- `docs/design/ai-native-roadmap.md` line 105 — the `@module purpose:
  "cooperative green-thread scheduler for CVM1"` annotation example
  (this ticket makes the described scheduler model concrete by adding
  the missing opcodes)
- CRUSH-20 (existing polyglot exec lane — DIFFERENT concern; this
  ticket is for **internal** concurrency, not external-language
  execution)
