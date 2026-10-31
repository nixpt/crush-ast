# .jagent/planning — crush-ast

Execution board. Pattern: squadron `.jagent/planning/`.

## Directory map

```
planning/
├── README.md           # this file
├── RULES.md            # standing discipline (verify before fix, one branch per unit, …)
├── ROADMAP.md          # milestones (M0–M11), phases, non-goals
├── MILESTONES.caison   # machine-readable milestone ledger (status, done_when, evidence)
├── BACKLOG-INDEX.md    # dispatch map: ticket IDs, ID ledger, gates, done status
├── tickets/            # one file per ticket: the source of truth for its Status
├── templates/
│   ├── ticket.md       # ticket template
│   └── issue.md        # bug report template
├── designs/            # design notes tied to tickets
├── research/           # spikes and measured baselines
├── archive/            # retired snapshots (e.g. TASKS-2026-10-14.md)
├── TASKS-root-archive-2026H1.md   # the pre-migration root TASKS.md, archived
├── SURVEY-2026-10-31.md           # status/ID/index reconciliation log
├── TASKS.md            # RENDERED, untracked (gitignored): `jagent-ticket render`
└── STATE.md            # RENDERED, untracked (gitignored)
```

Since `70a34d7` (2026-10-14) each ticket's own `Status` row is the source of
truth. `TASKS.md` and `STATE.md` are views rendered from the tickets plus open
dejavue captures (`jagent-ticket render`); they are gitignored, so don't edit
them by hand and don't commit them.

## How to use

1. **Start of session:** run `jagent-ticket render`, then read `TASKS.md`,
   `BACKLOG-INDEX.md` and the ticket you'll work on.
2. **Working:** create or fill a ticket file in `tickets/`. For dispatch-ready
   work, include a `## Dispatch` section with branch/runner/turn metadata.
3. **Closing:** update the ticket's `Status` row and add a `## Resolution`
   (RULES.md §1, §4); fix any index row (`BACKLOG-INDEX.md`, `ROADMAP.md`,
   `MILESTONES.caison`) that the change makes stale.
4. **Roadmap changes:** edit `ROADMAP.md` when milestones change.

## Status vocabulary

`Backlog` · `In Progress` · `Done` (the template's three) plus the terminal
`Superseded`, `Declined`, `Closed` and `Deferred`, which tickets already use.
Commentary may follow the keyword (`Done — PR #N merged …`). `In Progress` also
covers "partly landed, remainder open". Write the keyword first and plain (no
`**bold**`), so `jagent-ticket render` and greps classify it.

## Ticket naming

```
CRUSH-NNN-{slug}.md            # main numeric sequence
CRUSH-NNNa-{slug}.md           # residue/follow-up suffix of CRUSH-NNN
CRUSH-<lane>-NN-{slug}.md      # box/lane namespaces: CRUSH-zorro-NN, CRUSH-xps-NN
```

Never reuse IDs. Before minting, check `tickets/` on `origin/main` and on every
remote branch, plus open PR titles, because agents allocate concurrently. The
ID ledger in `BACKLOG-INDEX.md` records burned and renumbered IDs.
