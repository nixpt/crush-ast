# CRUSH-274 — The Crush lexer can't capture some `@lang { … }` bodies

| Field | Value |
|-------|-------|
| **ID** | CRUSH-274 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M6 |
| **Assignee** | — |
| **Dependencies** | — |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-16, found while building CRUSH-272 |

## Problem

`read_lang_body` (crates/crush-frontend/src/parser/lexer.rs) tracks string literals while
it looks for the block's closing `}`. Five programs of the JS walker's node corpus, used
as `@javascript` block bodies, fail to parse with `Unterminated string`: an apostrophe in
a `//` comment (`// \`var x;\` again … doesn't reset`), and an empty `''` string
(`console.log({ quote: "it's", empty: '', arr: [] })`). It is loud (a parse error), but
valid guest code is refused in both polyglot modes.

## Reproduction

```crush
fn main() {
    @javascript {
        // it doesn't matter
        console.log('', "x");
    }
}
```

Compile it: `Parse errors: Unterminated string …`.

## Technical approach

Per-language body scanning: skip `//` / `/* */` comments for JS and `#` comments for
Python, handle empty and escaped strings, triple-quoted Python strings, and JS template
literals with `${…}`. The node corpus (`crates/crush-lang-sdk/tests/polyglot_lower.rs`'s
`node_corpus_as_javascript_blocks_is_exact_or_loud`) then reaches 81 exact.
