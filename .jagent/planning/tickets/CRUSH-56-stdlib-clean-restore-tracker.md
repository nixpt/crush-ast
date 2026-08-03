# CRUSH-56 — STDLIB clean-restore tracker (103 caps; meta-ticket over CRUSH-88..97)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-56 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M9 |

## Problem

137 capabilities archived in `exosphere-1.0.zip`: 103 assessed clean-restorable,
46 mock-tainted (CRUSH-57's lane). ROADMAP's bar: **restore silent corruption
is worse than not restoring** — every restored cap must carry an M5 `@covers`
test proving behavior, zero mock markers. The blocking join (CRUSH-31
dejavue↔crush-index) is DONE (s412), so this can start.

## Approach

1. **Step 1 — locate + pin the source of truth**: find `exosphere-1.0.zip`
   and the "STDLIB RESTORATION MAP" (search crush-ast `stdlib/` + `docs/`,
   exosphere repo, assets/). If the per-cap map doesn't exist at cap
   granularity, CREATING it (cap name → clean/mock verdict → target module)
   is this ticket's first deliverable. Record location via dejavue.
2. Partition the 103 caps into the 10 shard tickets CRUSH-88..97 by family
   (io/fs/string/net/process/...); write each shard's cap list into its file.
3. Tracker table here: shard → caps → status; updated as shards land.
4. Restoration contract (applies to every shard): verbatim-restore ONLY if
   zero mock markers; `@covers` test per cap through the real pipeline;
   dejavue provenance line per shard.

## Definition of done

- [ ] Archive + map located/created and pinned (paths recorded)
- [ ] CRUSH-88..97 populated with real cap lists
- [ ] Tracker live; first shard (CRUSH-88) dispatched

## Files in scope

- `stdlib/`, `docs/`, this tracker, shard tickets

## Gates

None remaining (CRUSH-31 done). Gates CRUSH-88..97, CRUSH-57.
