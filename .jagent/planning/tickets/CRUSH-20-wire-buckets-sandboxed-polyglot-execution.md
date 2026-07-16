# CRUSH-20 — Wire buckets as a sandboxed polyglot execution path (spike already proven — this is the production wiring)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-20 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 (follow-on; larger than a typical M1 item, treat as its own mini-milestone) |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-19 recommended first (wall-clock timeout safety net); CRUSH-2 (polyglot capability gate) already done, this builds on it |
| **Estimated effort** | L |

## Problem — and the important context most sessions won't have

**This is NOT a research question anymore. It's already been spiked and empirically confirmed.**
Read `workspace-meta/plans/2026-07-14-crush-polyglot-via-buckets.md` (the full design + spike
writeup) before starting — this ticket is a compressed pointer to that doc plus the concrete next
steps, not a replacement for it.

Today, a granted `@python`/`@javascript`/`@bash` block (CRUSH-2's capability gate, already shipped)
still spawns the **host's own interpreter** with the host process's full ambient authority — the
capability grant authorizes the spawn, it does not sandbox it. The fix: provision an isolated,
bwrap-sandboxed runtime via `buckets` (already a real, working library in this workspace —
`projects/buckets`, not hypothetical) instead of touching the host interpreter at all.

**This was spiked and proven, twice, in-repo** (`agent/cece/CRUSHAST-BUCKETSPIKE-1`/`-2`, merged to
`main` this session): the standalone `crates/crush-bucketspike` crate + `SPIKE_RESULTS.md` +
`SPIKE_RESULTS_2.md` at the repo root are the receipts. Confirmed, not assumed:

- **bwrap sandboxing genuinely exercised** for all 3 languages (python/node/bash), verified by the
  *absence* of buckets' own "bwrap not found — running WITHOUT sandbox isolation" fallback warning
  across every one of 12+ independent sandboxed runs.
- **Marshaling survives the sandbox boundary intact** — the sentinel-line protocol
  (`CRUSH_RESULT_SENTINEL`, real `crush_vm::scheduler`/`crush_vm::vm::Value` types, not
  reimplemented) round-trips through bwrap's mount namespace with no truncation or corruption.
  Verified landing on a real decoded `Value::Int(6)`, not just "no crash."
  Env-var passthrough (how `EXEC_LANG` injects marshaled inputs today) also survives unchanged.
- **Real, measured latency** (not estimated): cold provisioning 347ms (bash) to 3.8–4.4s (python,
  first run only, network fetch); warm (cache hit) 7–47ms across all three — notebook-acceptable.

## What's actually left to build (the real scope of this ticket)

1. **Dependency-annotation syntax**: `@python[numpy, scipy] { ... }` — `LangBlock`'s CAST node
   needs a `deps` field; the parser needs to accept the annotation. Same "parser is the thin
   layer" pattern as the rest of this codebase's annotation handling.
2. **Layer-ownership decision**: does `crush-vm` grow a `buckets` dependency directly, or does
   `crush-lang-sdk` own the provisioning and hand crush-vm an already-resolved bucket? The design
   doc leans toward the SDK layer (keeps crush-vm dep-light; exo-light already mediates capsule
   provisioning separately via exo-hydra and the two provisioning paths shouldn't compete) — this
   needs an explicit decision before writing code, not a default.
3. **The numpy reframe (don't skip this)**: buckets provisions *bare language runtimes only* — no
   PyPI-level dependency resolution exists in it today. `@python[numpy]` is actually two
   differently-sized pieces: (a) bare python via buckets (proven, ready) and (b) `+numpy` = a
   *separate*, sandboxed `pip install numpy` inside the same bwrap env, needing
   `allow_network: true` plus a writable, persistent site-packages directory buckets doesn't
   manage today. Don't build (a) and assume (b) falls out for free.
4. **Actual wiring into `EXEC_LANG`'s spawn path** — the spike is a standalone proof crate
   (`crush-bucketspike`), not integrated into `crates/crush-vm/src/scheduler.rs`'s real `EXEC_LANG`
   handler. The production version swaps `Command::new(binary)` for a
   `buckets::resolve(...)` + `buckets::sandbox::sandboxed_command(...)` call (or the looser
   `firefly which <lang> --json` bridge if the library dependency turns out awkward — the design
   doc names both options).
5. **CRUSH-19 first (recommended)**: cold bucket provisioning can take up to ~4.4s measured; wiring
   this without a wall-clock bound on the capability-call path that would host it is exactly the
   scenario CRUSH-19 warns about. At minimum, whatever bucket-provisioning `HostCap`/opcode path
   this produces should internally reuse `run_with_wall_clock_limit`'s shape (already proven for
   `EXEC_LANG`) even if the generic `CAP_CALL` fix is deferred.
6. **Tie-in with CRUSH-18**: a sandboxed guest failure has a different shape than today's direct
   host-spawn failure — e.g. bwrap itself failing to start (sandbox setup error) is a distinct
   failure class from "the guest program raised its own exception inside a working sandbox" is
   distinct again from "the dependency resolve/fetch failed." Whatever error-mapping design
   CRUSH-18 lands on should be built with this 4th path's extra failure modes in mind, not
   retrofitted after.

## Non-goals

- Full PyPI/npm dependency-graph resolution (the numpy problem above) — start with bare-runtime
  sandboxing; treat `+deps` as an explicit, separately-scoped follow-on once (1)-(5) above are real.
- Porting exosphere's `spawn_native` (namespaces+cgroups+seccomp+GPU, Linux-only, daemon-coupled)
  into this path — that's a heavier, separate isolation tier for untrusted/heavy/GPU workloads per
  the design doc's tier table; this ticket is specifically the buckets/bwrap middle tier.
- Redesigning `exo-light`'s isolation backend selection — out of scope, separate repo, separate
  concern (the design doc's "Addendum" discusses the unification but doesn't require it be solved
  here).

## Files likely involved

- `crates/crush-vm/src/scheduler.rs` / `portable_vm.rs` — `EXEC_LANG` spawn path
- `crates/crush-frontend/src/parser/` — `@lang[deps]` annotation syntax
- `crates/crush-cast/src/*.rs` — `LangBlock`'s AST node, new `deps` field
- `projects/buckets` (sibling repo) — the actual sandbox library being integrated; may need API
  additions depending on what the wiring surfaces (check its current public API before assuming
  it's 100% sufficient as-is)
