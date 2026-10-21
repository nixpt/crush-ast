# CRUSH-280 — `cast_compile` bench fails: its CAST fixtures predate source positions in `meta`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-280 |
| **Priority** | P3 |
| **Status** | Done (this PR) |
| **Phase** | test hygiene |
| **Assignee** | kai |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | kai — 2026-10-20, from the note in PR #285 ("fails identically on unmodified bases") |

## Problem

`cargo bench -p crush-frontend --bench cast_compile` panics before it measures anything:

```
benches/cast_compile.rs:182:9: assertion `left == right` failed:
01 must be the verbatim parsed AST serialization
  left:  … "meta": {"col": 3, "line": 1} …     (Parser::parse today)
  right: … "meta": {} …                         (benches/fixtures/cast/01.cast.json)
```

The bench's setup asserts that each `benches/fixtures/cast/NN.cast.json` equals the serialization of
`Parser::parse(benches/fixtures/text/NN.crush)`. The fixtures were recorded on 2026-06-16 (`c9fdab4`, ported from
exosphere) with empty `meta`. Since then the parser records a source position (`line`, `col`) on every node, and
`Program` no longer serializes an empty `ai_meta`. Both are intended changes, so the fixtures are stale, not the
compiler. CI never runs benches, so nothing noticed. Reproduced on `main` at `eafd9f2`.

## Fix

- `CAST_FIXTURES_UPDATE=1 cargo bench -p crush-frontend --bench cast_compile -- --test` rewrites every fixture from
  the current parser (pretty JSON + newline, the existing format), then the normal run checks them. Same idea as
  api-drift's `API_DRIFT_UPDATE`: a deliberate serialization change is re-recorded on purpose and reviewed as a diff.
- The 20 fixtures are regenerated with it. Each CAST fixture is still checked against its text twin and still compiles:
  `cargo bench -p crush-frontend --bench cast_compile -- --test` → 40/40 `Success` (20 text paths + 20 CAST paths).

## Known wrinkle

`meta` is a `HashMap<String, serde_json::Value>` (`crush-cast`), so its keys don't serialize in a fixed order (`{"line", "col"}` in one node, `{"col", "line"}` in the next), so a
future regeneration can reshuffle them and make a noisy diff. The assertion compares `serde_json::Value`s, so order
doesn't affect pass/fail. If the noise matters, give `meta` an ordered map (or sort keys when writing fixtures).

## Not done

- Running the bench in CI (`--test` mode takes seconds). Worth adding to the feature-gates job so drift is caught
  when it happens; left for a CI change (needs a token with `workflow` scope).
