# CRUSH-83 — Compile cache / incremental unit: content-hash casm cache + per-function memoization

| Field | Value |
|-------|-------|
| **ID** | CRUSH-83 |
| **Priority** | P1 (headline perf-design ticket) |
| **Status** | Backlog |
| **Phase** | Design/perf (s412) |

## Problem

Panini capture (2026-08-02): every entry point recompiles from source, every
time (`crush-frontend/src/lib.rs:75-78`) — crushc, crush-run, the notebook
kernel, exo-light all pay full parse+analyze+compile per invocation. There is
no cache key, no incremental unit. (casm's dead `CachedProgram` was a stillborn
attempt — CRUSH-80.) For repeated-execution consumers (notebook cells,
exo-light fabric calls) this is among the largest wall-clock design wins
available.

## Approach

1. Design first (short doc): cache key = content hash of source + compiler
   version + feature flags; artifact = serialized casm Program (+ debug_info
   once CRUSH-74/79 land); location = per-user cache dir with the
   per-invocation-workdir publish pattern CRUSH-70 established (avoid its
   TOCTOU/collision bugs — cite CRUSH-67/70 fixes).
2. Wire at the shared compile entry so all consumers benefit; explicit
   `--no-cache` escape hatch.
3. Per-function memoization is phase 2 — only if the whole-program cache
   proves insufficient for the notebook's cell-edit loop.
4. Bench: cold vs warm compile of the largest example + a notebook-cell-edit
   simulation.

## Definition of done

- [ ] Design doc committed (key, invalidation, layout, concurrency story)
- [ ] Warm-path compile skips parse/analyze/compile (verified by bench delta,
      quoted before/after)
- [ ] Concurrent-invocation safety test (CRUSH-70 class)
- [ ] `cargo test --workspace` green; CRUSH-80 disposition honored

## Files in scope

- `crates/crush-frontend/src/lib.rs`, `crates/crush-lang-sdk` entry points; new cache module

## Gates

None hard. CRUSH-82 helps hashing cost; answers CRUSH-80's subsume question.


## Dispatch

Imported from `workspace-meta/prompts/crush-backlog/CRUSH-83.txt` on 2026-08-24 so this tracked ticket contains the dispatch prompt metadata.

- Branch: `agent/horse/CRUSH-83`
- Repo: `crush-ast`
- Turns: `120`
- Runner: `claude`
- Scope: this ticket file is the canonical implementation spec; read it before changing code.
- Discipline: verify the repro against current `main` first, commit and push each meaningful unit, and update this ticket with the result.
- Verification: satisfy this ticket's Definition of done, include test evidence, and quote the real post-commit `HEAD` hash.
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs` unless this ticket explicitly scopes them; flag `crush_cast::Function`/`Program` shape changes before landing.
- Halt: stop and DM foreman if gates are unmet, scope is wrong, the repro no longer exists, sandbox blocks required work, or budget is nearly exhausted.
