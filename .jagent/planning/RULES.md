# RULES — standing discipline for anyone working this backlog

Added s388 (2026-07-16). These are not suggestions — every agent (dispatched
horse, persona session, or the captain working directly) touching
`.jagent/planning/`'s backlog follows them. They exist because both failure
modes below have already happened in this workspace's history at large.

## 1. Verify before you fix

**A ticket's `Backlog` status is a claim, not a fact.** This session found two
`Priority: P0` tickets in this exact folder (`CRUSH-2` polyglot capability
bypass, `CRUSH-10` AOT-Rust backend) that were already fixed by unrelated work
— the ticket files just hadn't been updated. Before spending any effort on a
ticket:

1. Re-run its own `## Reproduction` section verbatim, against current `main`
   (not against the branch/worktree you're about to create — pull first).
2. If it no longer reproduces: update the ticket's `Status` to `Done` with a
   one-paragraph `## Resolution` section (what you ran, what it printed,
   which commit likely fixed it if findable) and mark the corresponding
   `TASKS.md` line `[x]`. Do not silently delete the ticket — the record that
   it was checked and closed is worth as much as the record that it was
   found.
3. If it does reproduce: proceed to fix it, and if the fix turns out
   different in shape than the ticket's `## Technical approach` guessed,
   that's fine — the approach section is a starting point, not a spec.

This applies recursively: if you find a NEW bug while working an existing
ticket, don't fold it silently into the same commit — file it as its own
`CRUSH-N` ticket (next available number) so it gets its own verify-before-fix
cycle later, and don't let it block the ticket you're actually on unless it
genuinely does.

## 2. One worktree + branch per milestone (or per ticket, whichever is smaller)

**Every unit of work gets its own worktree and its own branch.** "Unit of
work" means: one `CRUSH-N` ticket, or if you're working through several
small/related tickets as one coherent milestone (e.g. all of M1's black-box
correctness bugs), one milestone — but never more than one milestone's worth
of unrelated work on a single branch, and never work directly on `main`.

```bash
# per ticket:
git worktree add /home/nixp/worktrees/<agent>/CRUSH-N -b agent/<agent>/CRUSH-N origin/main

# per milestone (several related tickets):
git worktree add /home/nixp/worktrees/<agent>/M1-correctness -b agent/<agent>/M1-CORRECTNESS origin/main
```

Why per-milestone and not one giant branch for the whole backlog: a
milestone-sized branch is small enough for foreman/captain to actually
review and merge incrementally, and a bad turn on ticket 3 of 5 doesn't put
tickets 1-2's already-good work at risk of being rolled back together with
it. It also means the fleet can parallelize — a second horse can pick up M2
while the first is still mid-M1 without stepping on the same branch.

## 3. Commit + push at every milestone/phase boundary — don't batch to the end

**Push when a milestone (or ticket, if working ticket-by-ticket) is done —
not when the whole backlog is done.** Concretely:

1. Finish the milestone's work. Run its own verification (see the ticket's
   own success criteria, plus at minimum `cargo check` for the crates you
   touched and the relevant `cargo test -p <crate>`).
2. Commit with a message that names the ticket(s) closed.
3. Push the branch: `git push -u origin agent/<agent>/<MILESTONE-OR-TICKET>`.
4. Post to the bridge (`agent-msg "#general" "..."`) naming what shipped,
   what's verified, and what's next — so foreman can merge without having to
   reconstruct scope from the diff alone.
5. **Before starting the next milestone**, pull the latest `main` into a
   fresh worktree+branch (the previous milestone may have been merged by
   then, and the next milestone should build on top of that, not silently
   diverge from it).

Do not accumulate multiple milestones of work on one un-pushed branch and
hand it all over at the end — if turns/time run out mid-way, an unpushed
5-milestone branch is much harder to salvage than 3 pushed, mergeable
milestone branches plus a clearly-scoped 4th in progress.

## 4. Update `.jagent/planning/` as you go, not as an afterthought

- Mark `TASKS.md` checkboxes `[x]` the moment something is verifiably done —
  not at session close, not "later."
- Update the closed ticket's own `Status` field and add a `## Resolution`
  section (see §1) — future agents read the ticket file directly, they
  don't re-derive status from git log.
- If you file a new ticket (a bug found while working something else, per
  §1's recursive rule), use the existing template
  (`.jagent/planning/templates/ticket.md`) and the next available `CRUSH-N`
  number — check `.jagent/planning/tickets/` for the current max first.

## Cross-references

- `.jagent/planning/TASKS.md` — the current backlog these rules apply to
- `.jagent/planning/ROADMAP.md` — the milestone sequence
- `.jagent/planning/tickets/` — one file per `CRUSH-N`
- Workspace-level: `moms-kitchen` skill (worktree discipline generally),
  `foreman-merge-wave` skill (what foreman does with a pushed branch)
