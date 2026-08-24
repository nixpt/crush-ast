# CRUSH-76 — Fuzz the parser (lexer, parser, cson targets)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-76 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | Correctness spine (s412) |

## Problem

Zero fuzz targets exist (no `fuzz/` dir) — and that absence is exactly why the
cson bugs shipped (July research finding #5). The lexer's
unknown-char-becomes-Ident fallback (CRUSH-75) is the kind of latent behavior
fuzzing surfaces mechanically.

## Approach

1. `cargo-fuzz` targets: `fuzz_lexer` (bytes → lex, no panic), `fuzz_parser`
   (bytes → parse, no panic/OOM), `fuzz_cson` (parse+roundtrip: parse →
   serialize → parse, assert equal).
2. Seed corpus from the existing `.crush` files + cson fixtures.
3. CI smoke lane: short bounded run (e.g. 60s/target) on every push; longer
   runs stay local/manual.
4. Panics found → file as tickets (CAPTURE-ON-DISCOVERY), fix separately
   unless trivial.

## Definition of done

- [ ] Three targets build + run under `cargo fuzz`
- [ ] Seed corpus committed; CI smoke lane wired (respect the CRUSH-CI-CACHE-1
      warm-cache trap when adding the job)
- [ ] Any crashes found are filed as tickets with reproducer inputs

## Files in scope

- New `fuzz/` dir; `.github/workflows/ci.yml` (one job)

## Gates

None. After CRUSH-75 lands, re-seed with lambda corpus.


## Dispatch

Imported from `workspace-meta/prompts/crush-backlog/CRUSH-76.txt` on 2026-08-24 so this tracked ticket contains the dispatch prompt metadata.

- Branch: `agent/horse/CRUSH-76`
- Repo: `crush-ast`
- Turns: `60`
- Runner: `claude`
- Scope: this ticket file is the canonical implementation spec; read it before changing code.
- Discipline: verify the repro against current `main` first, commit and push each meaningful unit, and update this ticket with the result.
- Verification: satisfy this ticket's Definition of done, include test evidence, and quote the real post-commit `HEAD` hash.
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs` unless this ticket explicitly scopes them; flag `crush_cast::Function`/`Program` shape changes before landing.
- Halt: stop and DM foreman if gates are unmet, scope is wrong, the repro no longer exists, sandbox blocks required work, or budget is nearly exhausted.
