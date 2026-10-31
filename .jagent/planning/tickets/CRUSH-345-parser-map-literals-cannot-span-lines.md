# CRUSH-345 — Parser ends map literals at the newline

| Field | Value |
|-------|-------|
| **ID** | CRUSH-345 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M2 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

A `{ key: value }` map literal cannot span lines: the parser reports
`Expected property name but found newline` at the first line break after `{`,
so every non-trivial map must be written on one line — and a spec/config map
nested two levels deep becomes a 400-character line. Arrays span lines
happily (`[ {...},
      {...} ]` works), and map literals inside array elements work as long as
each individual map is one line.

Found while building `facet` (CLI spec maps) on 0.9.0.

## Reproduction

```bash
cat > /tmp/map.crush <<'EOF'
fn f() {
    return {
        ok: true,
        n: 1
    }
}
fn main() { print(f().ok) }
EOF
crush-run run /tmp/map.crush
# [compile] Parse errors: Expected property name but found newline at line 2, column 13,
#           Unexpected token at line 3, column 11: unexpected token in expression: `:` ...
# (checked on the v0.9.0 release binaries, 2026-10-31)
```

Both `return {` + newline and `let m = {` + newline fail identically; the
single-line form (`return { ok: true, n: 1 }`) is the only one accepted.

## Impact

- Library/package authors must flatten config maps; diffs on multi-line maps
  are unreadable and review-hostile.
- Suggests the lexer/parser treats `Newline` as a hard map terminator (or the
  map parser never consumes newlines), unlike arrays and call argument lists
  which do skip them.

## Success criteria

- [ ] `{` … newline … `key: value,` … `}` parses in every expression position
      (let, return, call argument, array element, nested).
- [ ] Trailing comma before `}` on its own line is accepted (or rejected with
      a targeted message, consistently for arrays and maps).
- [ ] A regression test covers multi-line maps nested inside a multi-line
      array of maps.

## Technical approach

- Inspect `map_literal` parsing in `crates/crush-frontend/src/parser/mod.rs`
  (Pratt prefix for `Token::LBrace`): skip `Token::Newline` where property
  names are expected, mirroring how array/argument parsing skips newlines.
- Check the lexer's newline handling around `LBrace` — if `{` pushes a
  "same-line" mode (struct-literal disambiguation vs `if cond {`), the map-literal
  arm needs an explicit newline-skip that doesn't break block statements.

## Files to modify

- `crates/crush-frontend/src/parser/mod.rs` — map-literal newline handling.
- `crates/crush-frontend/tests/compiler_tests.rs` — multi-line map fixtures.

## Non-goals

- Allowing bare `key: value` object syntax outside braces (JS-walker territory).
- Changes to expression-terminating newline rules elsewhere.
