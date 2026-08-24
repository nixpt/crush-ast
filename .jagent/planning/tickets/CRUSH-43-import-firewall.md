# CRUSH-43 — Import firewall: crush-pkg allowlist semantics

| Field | Value |
|-------|-------|
| **ID** | CRUSH-43 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M7 |

## Problem

No import mediation exists (zero allowlist/firewall hits in crush-pkg, s412
triage): any program can import any package. The ai-native/ops story
(untrusted agent-written programs) needs a manifest-declared allowlist.
ROADMAP flags a "spec early" placement risk: parse-time (crush-pkg) vs
enforcement-time (crush-lang-sdk host-cap provider) split must be designed
before code.

## Approach

1. Placement spec FIRST (short doc): crush-pkg parses `import` declarations +
   manifest allowlist; crush-lang-sdk's runtime cap provider enforces (parse
   alone is bypassable by constructed CASM). Decide default-open vs
   default-closed per manifest presence.
2. Implement: manifest schema, parse, enforcement hook, clear denial error.
3. Tests: allowed import works; denied import errors naming the firewall rule;
   no-manifest behavior matches the spec'd default.

## Definition of done

- [ ] Placement spec committed (docs/design + dejavue decision)
- [ ] Enforcement live at the runtime layer with tests (not parse-only)
- [ ] `cargo test -p crush-pkg -p crush-lang-sdk` green

## Files in scope

- `crates/crush-pkg`, `crates/crush-lang-sdk` cap provider, docs/design

## Gates

None.


## Dispatch

Imported from `workspace-meta/prompts/crush-backlog/CRUSH-43.txt` on 2026-08-24 so this tracked ticket contains the dispatch prompt metadata.

- Branch: `agent/horse/CRUSH-43`
- Repo: `crush-ast`
- Turns: `60`
- Runner: `claude`
- Scope: this ticket file is the canonical implementation spec; read it before changing code.
- Discipline: verify the repro against current `main` first, commit and push each meaningful unit, and update this ticket with the result.
- Verification: satisfy this ticket's Definition of done, include test evidence, and quote the real post-commit `HEAD` hash.
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs` unless this ticket explicitly scopes them; flag `crush_cast::Function`/`Program` shape changes before landing.
- Halt: stop and DM foreman if gates are unmet, scope is wrong, the repro no longer exists, sandbox blocks required work, or budget is nearly exhausted.
