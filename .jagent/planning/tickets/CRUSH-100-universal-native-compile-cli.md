# CRUSH-100 — Universal native compile CLI: mixed-language inputs → one .so

| Field | Value |
|-------|-------|
| **ID** | CRUSH-100 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M11 |

## Problem

The M11 headline UX: `crush compile *.crush hello.py lib.rs build.sh main.zig
--emit native` → a single .so. CRUSH-99 proves the 2-language mechanism; this
generalizes to N inputs across the walker set and gives it a CLI.

## Approach

CLI mode (extension-dispatched per input — note CRUSH-65's lesson: crushc
today parses ANY file as native crush regardless of extension; this CLI must
dispatch correctly and refuse unknown extensions loudly) → each input through
its walker → merged CAST program (symbol conflict rules defined + documented:
duplicate fn names across files = error naming both sites) → AOT-C → one .so.
Per-language fixtures; the 5-input ROADMAP example as the acceptance test.

## Definition of done

- [ ] The 5-input example compiles + runs correctly (differential vs PortableVm)
- [ ] Extension dispatch + loud refusal for unknown/ambiguous inputs (tests)
- [ ] Symbol-conflict semantics documented + tested

## Files in scope

- `crates/cli` / `crush-lang-sdk` (CLI), `crush-frontend` (program merge), `crush-aotc`

## Gates

CRUSH-99.


## Dispatch

Imported from `workspace-meta/prompts/crush-backlog/CRUSH-100.txt` on 2026-08-24 so this tracked ticket contains the dispatch prompt metadata.

- Branch: `agent/horse/CRUSH-100`
- Repo: `crush-ast`
- Turns: `80`
- Runner: `claude`
- Scope: this ticket file is the canonical implementation spec; read it before changing code.
- Discipline: verify the repro against current `main` first, commit and push each meaningful unit, and update this ticket with the result.
- Verification: satisfy this ticket's Definition of done, include test evidence, and quote the real post-commit `HEAD` hash.
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs` unless this ticket explicitly scopes them; flag `crush_cast::Function`/`Program` shape changes before landing.
- Halt: stop and DM foreman if gates are unmet, scope is wrong, the repro no longer exists, sandbox blocks required work, or budget is nearly exhausted.
