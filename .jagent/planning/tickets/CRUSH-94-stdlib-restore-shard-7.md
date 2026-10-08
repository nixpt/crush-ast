# CRUSH-94 — STDLIB clean-restore shard 7 of 10 (~10 caps)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-94 |
| **Priority** | P3 |
| **Status** | Superseded (2026-10-07) — CRUSH-122 + MIGRATION-INVENTORY §2.5 (CRUSH-169) |
| **Phase** | M9 |

## Scope

One shard of the 103-cap clean-restore campaign (tracker: CRUSH-56). Cap list
for this shard is written in by CRUSH-56 step 2 (partitioned by cap family
from the RESTORATION MAP); do not dispatch before that list exists here.

## Restoration contract (identical for every shard)

- Verbatim-restore ONLY caps with zero mock markers — if a mock marker turns
  up mid-restore, the cap moves to CRUSH-57's rewrite lane; never "fix it up
  inline".
- Every restored cap carries an M5 `@covers` test proving behavior through
  the REAL pipeline (parse → compile → execute), not a smoke test.
- One dejavue provenance line per shard: archive path, caps restored, caps
  bounced to CRUSH-57.
- Incremental commits: per-cap or per-2-3-caps, never one squash.

## Definition of done

- [ ] All listed caps restored with @covers tests green, or bounced to
      CRUSH-57 with reasons
- [ ] CRUSH-56 tracker row updated
- [ ] `cargo test --workspace` green

## Gates

CRUSH-56 (cap list + archive pinned).


## Dispatch

Imported from `workspace-meta/prompts/crush-backlog/CRUSH-94.txt` on 2026-08-24 so this tracked ticket contains the dispatch prompt metadata.

- Branch: `agent/horse/CRUSH-94`
- Repo: `crush-ast`
- Turns: `80`
- Runner: `claude`
- Scope: this ticket file is the canonical implementation spec; read it before changing code.
- Discipline: verify the repro against current `main` first, commit and push each meaningful unit, and update this ticket with the result.
- Verification: satisfy this ticket's Definition of done, include test evidence, and quote the real post-commit `HEAD` hash.
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs` unless this ticket explicitly scopes them; flag `crush_cast::Function`/`Program` shape changes before landing.
- Halt: stop and DM foreman if gates are unmet, scope is wrong, the repro no longer exists, sandbox blocks required work, or budget is nearly exhausted.

## Resolution — Superseded (2026-10-07, CRUSH-169)

Closed by foreman-approved planning hygiene (CRUSH-169, lane E2 of the phase-2 relay). The
`exosphere-1.0.zip` restore plan is superseded:

- `exosphere-1.0.zip`'s `crates/core/base/stdlib` is the same crate as exosphere's live tree, and its
  `archive/archived-stdlib` an older snapshot of it. **CRUSH-122** already restored the clean families
  from the live tree (collections, bytes/buffer, binary, result, text.sort/uniq, time.format/parse,
  env.os/arch, `system.*` SBL) with tests.
- The "46 mock-tainted" caps are exactly the families `docs/planning/MIGRATION-INVENTORY.md` §2.2
  classifies **dead/out** (polyglot bridge, ai/agent/learn mocks, dom, task, data.*, print/text.echo).
- The genuine remainder is filed as **CRUSH-151..155** (fs coreutils, async.sleep, env.all/home_dir +
  http verbs, storage.* decision, effects metadata).

See MIGRATION-INVENTORY §2.3–§2.5. Ticket file kept (not deleted) for history.
