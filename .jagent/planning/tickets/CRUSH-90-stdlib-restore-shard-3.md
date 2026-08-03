# CRUSH-90 — STDLIB clean-restore shard 3 of 10 (~10 caps)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-90 |
| **Priority** | P3 |
| **Status** | Backlog — cap list assigned at CRUSH-56 execution |
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
