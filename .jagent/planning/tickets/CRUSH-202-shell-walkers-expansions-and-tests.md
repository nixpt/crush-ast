# CRUSH-202 — Bash/Zsh walkers: `$((…))`, `$(…)`, `${#s}` printed literally; `[ a -op b ]` always true

| Field | Value |
|-------|-------|
| **ID** | CRUSH-202 |
| **Priority** | P0 |
| **Status** | Backlog |
| **Phase** | M6 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

- `a=7; echo $((a+1))` → `$((a+1))`; `v=$(echo hi); echo $v` → `$(echo hi)`;
  `s=hello; echo ${#s}` → `${#s}`.
- Zsh: `x=15; if [ $x -gt 20 ]; then echo big; fi` → `big`. The closing `]` makes 4
  args, which falls through to `BoolLiteral true` (`crush-lang-zsh/src/lowerer.rs:~743-771`).
  `while [ … ]` loops forever for the same reason.
- Bash: `[ -lt ]` unsupported; `echo a b` prints `ab`; function arguments broken.
- Zsh expands variables inside single quotes.

## Success criteria

- [ ] Arithmetic, command substitution and length expansions lowered (or rejected loudly).
- [ ] `[ … ]`/`test` comparison operators correct; unknown forms are errors, not `true`.
- [ ] Output tests against `bash` (and `zsh` where installed).
