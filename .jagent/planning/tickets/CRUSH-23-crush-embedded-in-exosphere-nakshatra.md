# CRUSH-23 — Crush embedded/building inside exosphere and nakshatra (capture only — not scoped, not started)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-23 |
| **Priority** | Aspirational (not scheduled) |
| **Status** | Captured |
| **Phase** | none — post-M1 idea, overlaps existing M4 "cross-project integration" milestone |
| **Assignee** | unassigned |
| **Dependencies** | `exosphere/.jagent/planning/specs/EXO-194-crush-engine-convergence.md` (DECIDED, 2026-07-05) is the authoritative map for the exosphere half — read it first, don't re-derive |
| **Estimated effort** | unscoped |

## Origin

Captain, s388: capture a ticket for "crush builds inside exosphere/nakshatra." Capture-only, same
as CRUSH-21/22 — no code, no design decision. **Important: this is NOT a blank-slate question for
the exosphere half** — `EXO-194` already researched and decided most of it (2026-07-05, captain).
This ticket exists to (a) point at that spec rather than duplicate it, and (b) add the nakshatra
half, which EXO-194 explicitly notes but does not resolve.

## Current state, verified (not assumed)

**Exosphere already runs Crush two different ways** (full detail in EXO-194 — summarized here):
- **CVM1** (`crush-ast/crates/crush-vm`, binary `CVM1`-magic bytecode, `HostCap` object-caps) — the
  **go-forward path**, reached via `exo-light`'s `FabricExecutor`. Path-dep confirmed:
  `exosphere/crates/core/crush-symbols/Cargo.toml` depends on
  `crush-ast/crates/tree-sitter-crush` directly — the one concrete cross-repo dependency edge
  today between exosphere and crush-ast.
- **exosphere's own in-tree, pre-extraction fork** (`crush-lang`/`crush-cast`/`casm`/`nanovm` under
  `exosphere/crates/core/*`) — **frozen as of EXO-194's decision**: no new consumers, ~25 existing
  dependents left alone to migrate opportunistically or not at all.
- EXO-194's load-bearing finding: **CVM1 has no language compiler of its own** — every real Crush
  program compiles to a `casm::Program` first, and neither of the two `casm::Program` variants
  (crush-ast's vs. exosphere's own, a genuine crate-name collision — same name, different types)
  runs on CVM1 today. "Move everything to CVM1" is new code (a casm→CVM1 lowerer), not a config
  flip — EXO-194 already flags this, doesn't solve it.

**Nakshatra has no Crush engine of its own** (EXO-194's own words: "nakshatra has no Crush engine
at all... the 'nakshatra version' in the original framing does not exist"). Nakshatra's own
doctrine (`docs/EXOSPHERE-COVERAGE.md`) is explicit: nakshatra "does not touch exosphere and does
not reimplement the userspace PID-1 stack" — it owns the kernel layer only and is meant to *reuse*
exosphere's userspace, not grow a parallel one.

**But nakshatra does have one real, working Crush artifact today**: `tools/build.crush` — a
build-orchestration script (config → olddefconfig → compile steps, each step capability-gated via
stubbed `run_tool`/`emit` cap_calls) that the repo's own README documents as
"Validated through crush-run (parse → typecheck → compile-to-CASM → nanovm)... built from
`crates/core/crush-lang`" (see `tools/README.md`, committed `45d5b6d`). **This runs on exosphere's
in-tree/frozen engine (path C in EXO-194's table), not the CVM1/crush-ast path exo-light exposes.**
Timing check: `build.crush` was validated 2026-06-11 — a month *before* EXO-194's 2026-07-05 freeze
decision, so this is not a rule violation, just a pre-existing fact worth knowing before anyone
assumes nakshatra's Crush usage is greenfield.

## The open question this ticket actually adds (beyond EXO-194)

EXO-194 settled exosphere's internal two-engine question with "leave both, converge passively."
It explicitly did NOT settle what nakshatra should do, because at the time nakshatra had no engine
to weigh in on — but nakshatra *does* have a live, working Crush artifact now, running on the
specific engine (the frozen one) that EXO-194 says new consumers shouldn't add to. Two honestly
open branches, not resolved here:

1. **Nakshatra keeps reusing whatever exosphere settles on** (its own stated doctrine — "reuse, not
   reimplement"), which today means staying on the frozen in-tree engine for `build.crush`
   specifically, since that's what's already validated and working. Consistent with nakshatra's
   architecture, but means nakshatra's one Crush use case sits on the path exosphere itself is
   letting decay.
2. **Nakshatra's use case (build orchestration, not general app/capsule execution) may not need
   the full engine debate at all** — it's a narrower need (compile-and-run one workflow script at
   build time, not host arbitrary capsules at runtime) than what CVM1 vs. in-tree-fork is actually
   arguing about. Worth asking whether `build.crush` even wants a runtime engine dependency at all,
   versus e.g. a minimal `crush-run`-shaped CLI binary — genuinely unresolved, not answered here.

## Non-goals (for this ticket, right now)

- No re-litigating EXO-194's decision — it stays DECIDED; this ticket doesn't propose reopening it.
- No casm→CVM1 lowerer design, no crate-rename plan for the `casm` name collision — those live in
  EXO-194 already if/when someone picks that arc back up.
- No claim that nakshatra's `build.crush` is broken or needs migrating right now — it works today,
  validated, on the engine it was written against.

## Cross-references

- `exosphere/.jagent/planning/specs/EXO-194-crush-engine-convergence.md` — **read this first**;
  authoritative for the exosphere-internal half of this question.
- `exosphere/crates/core/crush-symbols/Cargo.toml` — the one live exosphere→crush-ast dependency
  edge (`tree-sitter-crush`, path-dep).
- `nakshatra/docs/EXOSPHERE-COVERAGE.md` — nakshatra's own "reuse, don't reimplement" doctrine
  toward exosphere's userspace.
- `nakshatra/tools/build.crush`, `nakshatra/tools/README.md` — the one real, working Crush artifact
  in nakshatra today, and exactly which engine it validates against.
- `.jagent/planning/TASKS.md` M4 milestone ("cross-project integration... exosphere reconciled") —
  this ticket sits under the same milestone; nakshatra is the piece M4 didn't previously name.
- `.jagent/planning/tickets/CRUSH-21-java-kotlin-language-family.md`,
  `CRUSH-22-build-platforms-and-architectures.md` — the other two capture-only tickets filed this
  session, same "document, don't design" scope.
