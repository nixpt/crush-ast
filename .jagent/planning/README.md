# .jagent/planning — crush-ast

Execution board. Pattern: squadron `.jagent/planning/`.

## Directory map

```
planning/
├── README.md           # this file
├── STATE.md            # current project state + metrics (per-session updates)
├── ROADMAP.md          # milestones (M0-M11), phases, non-goals
├── TASKS.md            # milestone/backlog status summary
├── BACKLOG-INDEX.md    # dispatch map: ticket IDs, prompt links, gates, done status
├── tickets/            # ticket files; imported prompts live in ## Dispatch sections
└── templates/
    ├── ticket.md       # ticket template
    └── issue.md        # bug report template
```

## How to use

1. **Start of session:** Read `STATE.md` → `TASKS.md` → pick from P1/P3/P4
2. **Working:** Create/fill a ticket file in `tickets/`. For dispatch-ready work, include a `## Dispatch` section with branch/runner/turn metadata.
3. **End of session:** Update `STATE.md` metrics + test count. Move completed items.
4. **Roadmap changes:** Edit `ROADMAP.md` when milestones change.

## Ticket naming

```
CRUSH-NNN-{slug}.md
```

Where NNN = sequential number, slug = short name. Start from CRUSH-1. Never reuse IDs.
