# CRUSH-275 — Define the Crush 1.0 release criteria

| Field | Value |
|-------|-------|
| **ID** | CRUSH-275 |
| **Priority** | P2 |
| **Status** | In Progress (draft for captain review) |
| **Phase** | Publish → 1.0 |
| **Assignee** | vega (draft) / nixpt (decisions) |
| **Dependencies** | none |
| **Estimated effort** | M |

## Problem

The captain wants to release Crush 1.0.0. A 1.0 is a compatibility promise: after it, a breaking change
to anything promised needs 2.0. Nothing in the repo yet says which surfaces would be frozen, what must be
true first, or how 1.0 would be reached. Meanwhile M2/M3/M4/M5 are active, M6/M9 are planned and COSM is
only partly built.

## Success criteria

- [x] Draft `docs/planning/RELEASE-1.0-CRITERIA.md`: the promise and its non-scope, a checklist of
      testable criteria with current status and evidence, the full/partial scope options, the release
      train, the deprecation policy, the post-1.0 versioning rules, and open questions.
- [ ] The captain picks a scope (full / partial-by-surface / language-only) and answers the open
      questions in §5.
- [ ] Accepted criteria are filed as tickets, or linked to existing ones, and assigned to the 0.8 / 0.9
      / rc releases.
- [ ] Urgent standalone fix filed: `bump-version.sh` must not mint 1.0.0 from a `feat!:` commit while
      major = 0 (criterion R1).

## Non-goals

- Changing versions, tags, CHANGELOG, or the publish/release workflows.
