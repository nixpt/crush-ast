# CRUSH-86 — Dead crush-vm deps in clients: squeeze + crush-visuals-debug-bridge

| Field | Value |
|-------|-------|
| **ID** | CRUSH-86 |
| **Priority** | P3 (hygiene; payoff = accurate client ripple graph) |
| **Status** | Backlog |
| **Phase** | Hygiene (s412) — CROSS-REPO |
| **Repo** | `crush-workspace` (both edits; work dispatches there) |

## Problem

Panini client-survey captures (2026-08-02), merged into one ticket:
1. `squeeze` declares a `crush-vm` dependency with zero usage (Cargo.toml:32).
2. `crush-visuals-debug-bridge` declares `crush-vm` but only uses
   `crush_debugger`.
Verify both in `/home/nixp/WORKSPACE/projects/crush-workspace`. Dead deps
inflate every "who consumes crush-vm" ripple analysis (exactly the survey
CRUSH-71 ran) and add build weight to clients.

## Approach

Remove the dep from squeeze; replace with `crush-debugger` (direct) in
crush-visuals-debug-bridge. `cargo build` + tests in each crate; grep for any
feature-gated usage before deleting (negative grep is not absence — read the
crates' src).

## Definition of done

- [ ] Both Cargo.tomls corrected; both crates build + test green
- [ ] One-line note in each commit naming this ticket

## Files in scope

- `crush-workspace/squeeze/Cargo.toml`, `crush-workspace/crush-visuals/crates/crush-visuals-debug-bridge/Cargo.toml`

## Gates

None.


## Dispatch

Imported from `workspace-meta/prompts/crush-backlog/CRUSH-86.txt` on 2026-08-24 so this tracked ticket contains the dispatch prompt metadata.

- Branch: `agent/horse/CRUSH-86`
- Repo: `crush-workspace`
- Turns: `30`
- Runner: `claude`
- Scope: this ticket file is the canonical implementation spec; read it before changing code.
- Discipline: verify the repro against current `main` first, commit and push each meaningful unit, and update this ticket with the result.
- Verification: satisfy this ticket's Definition of done, include test evidence, and quote the real post-commit `HEAD` hash.
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs` unless this ticket explicitly scopes them; flag `crush_cast::Function`/`Program` shape changes before landing.
- Halt: stop and DM foreman if gates are unmet, scope is wrong, the repro no longer exists, sandbox blocks required work, or budget is nearly exhausted.
