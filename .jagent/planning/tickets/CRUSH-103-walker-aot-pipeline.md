# CRUSH-103 — Walker→AOT pipeline for all 12 walkers (ex-CRUSH-39, renumbered)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-103 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M6 |

## Problem

ROADMAP proposed this as CRUSH-39, but branch `agent/panini-crush/CRUSH-39`
burned that ID with unrelated stash-shaped Math.* WIP (salvage-only; the work
became CRUSH-65/69) — renumbered 103 to keep records unambiguous. Current
state (s412 triage): NO walker crate has an AOT path (`grep -l aot` over
crates/crush-lang-*/src hits only crush-lang-sdk/differential.rs; verify).
Repo-root TASKS.md claims c/python/basic-js had "direct AOT paths" — verify
what that actually meant; the walker→CAST→CASM→AOT chain is the goal.

## Approach

The pipeline is shared plumbing, not 12 implementations: walker → CAST is
done per-walker (M6's parity work); CAST → CASM → AOT is engine-side and
language-agnostic. So: (1) prove the chain end-to-end for c/python/js
(fixtures compiled to native + executed, differential-checked vs PortableVm);
(2) turn the remaining 9 walkers green mostly by fixture coverage; (3) per-
walker gaps found → filed per RULES (own tickets if non-trivial).

## Definition of done

- [ ] c/python/js: source → native binary → correct output, in CI
- [ ] All 12 walkers: chain attempted with per-walker status table (green or
      filed-gap — no silent skips)
- [ ] Differential fixtures for the green set

## Files in scope

- `crates/crush-aot`/`crush-aotc` entry plumbing, per-walker fixtures

## Gates

CRUSH-35 (parity), CRUSH-36 (adapter unification). Gates CRUSH-98.
