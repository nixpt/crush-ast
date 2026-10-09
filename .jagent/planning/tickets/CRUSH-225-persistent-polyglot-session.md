# CRUSH-225 — Design: polyglot session state across blocks (one worker per language per run)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-225 |
| **Priority** | P2 |
| **Status** | Backlog (design first) |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-224 (imports); read alongside CRUSH-166 |
| **Estimated effort** | M (design note S, then implementation) |
| **Filed by** | kai (foreman) — s476, 2026-10-09 |

## Problem

Every `@lang` block runs in a fresh subprocess, so nothing set in one block survives to
the next:

```crush
@python {
import json
}
@python {
print(json.dumps([1, 2]))
}
# NameError: name 'json' is not defined
```

CRUSH-224 fixes declared imports at compile time. Guest *state* (globals, loaded
models, open connections) still resets per block, so any expensive setup is paid
once per block.

Prior art in exosphere, tested on its 2026-10-08 release build:

- In-process Lua (nanovm builtin, mlua) kept globals across blocks: `x = 41` then
  `print(x + 1)` → `42`.
- JS (`ReferenceError`) and Python (`NameError`) did not persist there either.
- exosphere's `crush-python` has a `python_worker` subprocess pool
  (`crates/platform/runtimes/python/src/worker.rs`, binary in `src/bin`). This is the
  model that fits crush-ast's rules.

## Constraints

- C-2 / C-8 stand: no interpreter running inside the VM process. The worker stays a
  subprocess, provisioned and sandboxed through buckets like today.
- Determinism and capability gating must not regress. A session must not outlive
  the run, and must not be shared across runs or programs.

## Design questions to answer

1. **Opt-in or default?** Options: `@python(session) { … }`, a program-level
   `use @lang python session`, or always on for a run.
2. **Protocol:** stdin/stdout framed JSON (variables in, result + sentinel out),
   reusing `value_to_polyglot_env` / `CRUSH_RESULT_SENTINEL` semantics.
3. **Lifecycle:** spawn on first block, kill at VM halt/error/timeout. Process group
   is killed as in `run_with_wall_clock_limit`. The per-block wall-clock limit still
   applies.
4. **Failure:** a guest exception leaves the worker usable; a worker crash fails that
   block loudly and the next block respawns.
5. **Which engines:** interp scheduler and PortableVm share `run_exec_lang`. Decide
   whether FastVM/JIT/AOT get it or reject session blocks.
6. **Overlap with CRUSH-166:** guest→host callbacks need the same long-lived channel.
   Design both on one protocol.

## Success criteria

- [ ] Design note `docs/design/polyglot-session.md` answering 1–6, ratified.
- [ ] Then: the repro above prints `[1, 2]` under the chosen opt-in, on both shared
      engines, with a test for crash/respawn and for kill-at-halt.

## Related

- CRUSH-224, CRUSH-166, `docs/design/exec-lang-pluggable-executor.md` (an embedder
  could supply the session executor).
