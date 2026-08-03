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
