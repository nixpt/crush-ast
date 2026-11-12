# CRUSH-394 — `regex.match` unusable: `match` keyword rejected after `.`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-394 |
| **Priority** | P1 |
| **Status** | Done |
| **Phase** | M2 |
| **Assignee** | opencode |
| **Dependencies** | none |
| **Estimated effort** | S |

**Filed by:** opencode — 2026-11-10, from GitHub issue #367; renumbered from CRUSH-367 (ID collision with CRUSH-367-crush-pkg-imports-through-the-package.md, per CONTRIBUTING's renumber-the-newer rule)

## Problem

`regex.match` is listed by `crush-run caps` (regex.test/match/find_all/replace/split)
but cannot be called: `match` lexes as the keyword, so the parser rejects it as a
field name after `.`:

```crush
fn main() {
    print(regex.match("^[a-z]+", "ab cd"))
}
```

```text
[compile] Parse errors: Expected field name but found `match` at line 2, column 17
```

Expected: a capability/field name after `.` may be any identifier-shaped word,
keywords included (or rename the capability).

## Success criteria

- [x] `regex.match(...)` (and any other keyword-named member access, e.g. `obj.if`, `obj.for`) parses.
- [x] `match` remains a keyword everywhere else: match expressions, bare call targets, statement starts.
- [x] The tree-sitter grammar (`crates/tree-sitter-crush`) agrees — verified it already lexes a keyword after `.` as `identifier` (context-sensitive lexing); corpus case added to pin it.
- [x] End-to-end repro: `crush-run run` of the issue's program compiles, runs, and prints `[ab]` (the capture-groups array `regex.match` returns per `stdlib.rs` `RegexMatchCap`).
- [x] `cargo test -p crush-frontend` (108 lib + integration suites) and tree-sitter grammar tests green (corpus 19/19, `grammar_examples.rs` 2/2); no new clippy warnings.

## Technical approach

- `crush-frontend` parser: the field-access branch of `parse_expression_with_precedence` now accepts keyword tokens after `.`, via a new `Token::keyword_name()` helper that maps each keyword token back to its spelling. After a dot the word cannot start a statement or expression, so it is unambiguous. Literal keywords (`true`/`false`/`null`) stay rejected as field names; multilingual keyword spellings read back as their canonical English form (the token doesn't record the spelling).
- `GetField`+call lowering already turns `obj.field(...)` into `CapabilityCall { name: "obj.field" }`, so `regex.match(...)` needs no stdlib or lowering change — only the parse door.
- Tree-sitter: regenerated with CLI 0.25.10 to confirm `grammar.js` ↔ `parser.c` are in sync (only the known `parser.h` typedef-drop; kept at HEAD per CRUSH-221's convention). No grammar rule change was needed. Corpus case `test/corpus/keyword_fields.txt` added so the agreement is pinned, not incidental.

## Resolution (2026-11-10, opencode)

Fixed in `agent/opencode/CRUSH-367`:

- `crates/crush-frontend/src/parser/lexer.rs` — `Token::keyword_name()` helper (keyword → spelling; consulted only after `.`).
- `crates/crush-frontend/src/parser/mod.rs` — field access after `.` accepts keyword-shaped names; parser unit tests (`test_keyword_field_access_after_dot`, `test_keyword_field_access_other_keywords`, `test_keyword_still_keyword_outside_field_access`).
- `crates/tree-sitter-crush/test/corpus/keyword_fields.txt` — corpus case pinning grammar agreement.

Verification (captured output):

```console
$ cargo run -q -p crush-lang-sdk --bin crush-run -- run /tmp/repro367.crush < /dev/null
[ab]
```

(`regex.match` returns the capture-groups array per the stdlib implementation —
`["ab"]` for `^[a-z]+` on "ab cd" — printed as `[ab]`. `regex.test` is the
capability that returns 1/0.)

Example-corpus check: parse-failure set over `examples/crush/*.crush` is
IDENTICAL before/after the change (2 files, both documented expect-error
examples). `cargo clippy -p crush-frontend --all-targets`: no warnings from the
new code.

## Non-goals / notes for issue #362

- `m.match(1)` on a map now PARSES (previously a parse error) and lowers to
  `CapabilityCall { name: "m.match" }` — the same lowering every
  `obj.field(...)` call already gets. The capability gate then refuses it
  explicitly at load time ("needs capabilities this run does not grant … not
  available in this host: match"), so nothing runs silently. Bare member access
  `m.match` on a map works as plain `get_field` (returns the stored value).
  Whether member calls on values should dispatch to members instead of
  capabilities remains issue #362's turf — no lowering change made here.
- The `regex.match` capability was NOT renamed; the stdlib registration
  (`crush-lang-sdk/src/stdlib.rs` `RegexMatchCap`) is untouched.
- Repo-wide formatting was left to the separate formatting ticket.
