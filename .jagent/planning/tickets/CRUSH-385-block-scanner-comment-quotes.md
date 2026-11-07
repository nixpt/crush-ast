# CRUSH-385 — polyglot blocks: quotes and braces in comments break the block scanner

| Field | Value |
|-------|-------|
| **ID** | CRUSH-385 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | Polyglot |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |
| **Filed by** | altair — 2026-11-07, found by the CI failure on PR #405 |

## Problem

A valid polyglot block can fail to compile because of what is in a comment. `read_lang_body` in `crates/crush-frontend/src/parser/lexer.rs` finds the closing `}` by tracking braces and quotes, and it treats every language the same. It doesn't know about comments, so an apostrophe in `# isn't` opens a string that never closes, and a brace in a comment changes the depth.

The failure is a compile error, not wrong output. The error points at the comment's quote, but the message doesn't say the comment is the cause.

## Evidence

Compiled through `compile_crush_source_with` with `PolyglotMode::Lower`, at `main` `082c21c2` (v0.11.1 plus PR #404). Each body sat in a `@lang -> done { ... }` block inside `main`.

| Lang | Body | Result |
|---|---|---|
| python | `# isn't` | `Unterminated string` |
| python | `# 5" wide` | `Unterminated string` |
| python | `# it '''` | `Unterminated string` |
| python | `# {` | `Expected } but found end of input` |
| python | `# }` | `Unexpected token`: block closes early |
| python | `# isn't` twice | Compiles: the two apostrophes pair up |
| python | `print("isn't")` | Compiles |
| javascript | `// isn't` | `Unterminated string` |
| javascript | `/* isn't */` | `Unterminated string` |
| javascript | `console.log(/'/.test('a'));` | `Unterminated string` |

The pairing case compiles, but everything between the two apostrophes counts as string, so a brace in that span isn't tracked. Whether that can produce a wrong block boundary with no error is untested.

The last row isn't a comment. A JS regex literal has the same cause, and fixing comments alone won't clear it.

PR #405 hit this through `cpython_corpus_as_python_blocks_matches_cpython`: `isn't` in a comment in `25_chained_compare.py`. That PR reworded the comment, so this bug is still there.

## Scope

Fixing this means passing the block's language to `read_lang_body` so it can skip that language's comments. Other languages' comment syntax needs a look. For a language with no entry, the current behaviour should stay.

JS regex literals need separate handling, because telling a regex from division takes more than a character scan. They can stay out of this ticket if that gets too large, but then they need their own ticket.

## Success criteria

- [ ] Every comment row in the Evidence table compiles
- [ ] A brace in a comment doesn't change the block boundary
- [ ] Quote and brace handling inside real strings is unchanged, including Python triple quotes and JS backticks
- [ ] Tests cover Python and JS comment forms through the `PolyglotMode::Lower` route
- [ ] The regex row is fixed or has its own ticket
