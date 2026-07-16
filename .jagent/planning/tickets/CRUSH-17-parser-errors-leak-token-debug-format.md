# CRUSH-17 — Parser error messages leak `Token`'s Debug representation instead of a human-readable name

| Field | Value |
|-------|-------|
| **ID** | CRUSH-17 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

The good news first: crush's parse-error diagnostics are genuinely well-built
— `crush-lang-sdk::theme::render_parse_error` produces a real, rustc-style
diagnostic with a stable `E-PPnn` code, `file:line:col`, and a colored source
snippet with a caret pointing at the exact column. This already works
end-to-end (verified s388, see reproduction below) and is not what this
ticket is about.

The bug is in the message *text* itself. Every parser error site that
reports "found token X" builds that text via `format!("{:?}", self.peek())`
— Rust's derived `Debug` impl for `Token`, which is a 69-variant enum where
**every variant carries an embedded `SourceLocation` struct**. So instead of
a clean token name, the user sees the enum variant name AND a second,
redundant copy of the line/col info already shown in the diagnostic header,
formatted as raw Rust struct-debug syntax:

```
[E-PP01] /tmp/idx_assign.crush:3:11: Unexpected token in expression: Assign(SourceLocation { line: 3, col: 11 })
```

Compare to what this should read like (rustc-equivalent quality):

```
[E-PP01] /tmp/idx_assign.crush:3:11: unexpected token in expression: `=`
```

This is not one stray call site — `grep -c '{:?}", self.peek()' parser/mod.rs`
returns **30 occurrences** across the parser, every one of them building
either a `ParseError::UnexpectedToken.msg` or a `ParseError::Expected.found`
field the same broken way.

## Impact

Every single parse error a Crush programmer sees has a message half-composed
of an internal enum's Debug dump. It doesn't block anything (the location
info is still separately correct and prominent), but it's the exact opposite
of what a language's error-reporting quality bar should look like, and it's
systemic rather than a one-off typo — 30 sites, one root cause (`Token` has
no human-readable formatter at all, so every call site reached for `{:?}`
by default).

## Reproduction

```bash
cat > /tmp/idx_assign.crush <<'EOF'
fn main() {
    let xs = [5, 5, 5];
    xs[0] = 9;
}
EOF
crushc /tmp/idx_assign.crush -o /tmp/idx_assign.cvm1
```

Actual output (the location/snippet rendering is correct; the message text
is the bug):

```
[E-PP01] /tmp/idx_assign.crush:3:11: Unexpected token in expression: Assign(SourceLocation { line: 3, col: 11 })
--> /tmp/idx_assign.crush
  1 | fn main() {
  2 |     let xs = [5, 5, 5];
> 3 |     xs[0] = 9;
  4 | }
    |           ^
```

## Technical approach

1. Add a human-readable formatter for `Token` — either implement `Display`
   (returning e.g. `"="` for `Assign`, `"identifier"` / `` `foo` `` for
   `Ident`, `"end of input"` for `EOF`, etc.) or a dedicated
   `fn describe(&self) -> String` / `&'static str` method alongside the
   existing `Debug` derive (don't remove `Debug` — it's still useful
   internally, e.g. compiler-internal panics/asserts).
2. Replace all 30 `format!("{:?}", self.peek())` call sites in
   `crates/crush-frontend/src/parser/mod.rs` with the new formatter.
3. Update `parse_error_triple`/`render_parse_error` if needed (likely no
   change required — they already just interpolate the `msg`/`found`
   string, the fix is entirely at the construction site).
4. Update the 3 existing `theme.rs` tests that assert on rendered error
   text (`out.contains("[E-PP03]")` etc.) if any of them incidentally
   depend on the old Debug-formatted text — check before assuming none do.

## Files to modify

- `crates/crush-frontend/src/parser/lexer.rs` — add the `Token` formatter
  (natural home, next to the `Token` enum definition)
- `crates/crush-frontend/src/parser/mod.rs` — the 30 call sites

## Non-goals

- Redesigning the diagnostic *rendering* (file:line:col + snippet) — that
  part already works well and is out of scope here
- Touching the lexer's own error variants (`UnterminatedString`,
  `InvalidNumber`) — those don't go through `self.peek()`, not affected
