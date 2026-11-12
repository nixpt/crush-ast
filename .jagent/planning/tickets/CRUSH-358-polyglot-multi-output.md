# CRUSH-358 — Polyglot blocks with 2+ outputs (`@python -> a, b { … }`) fail to parse

| Field | Value |
|-------|-------|
| **ID** | CRUSH-358 |
| **Priority** | P1 |
| **Status** | Done — branch `agent/opencode/CRUSH-358`, 2026-11-10 |
| **Phase** | M1 |
| **Assignee** | opencode |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | opencode — 2026-11-10, from GitHub issue #358 |

## Problem

A polyglot block that declares **two or more outputs** in the documented bare
form fails to parse; one output works:

```crush
fn main() {
    @python -> a, b {
        a = 1
        b = 2
    }
    print(a + b)
}
```

```text
[compile] Parse errors: Unexpected token at line 2, column 5: unexpected token in expression: `@python`, …
```

The multi-output form is documented (`crates/crush-vm/src/lang_exec.rs`,
`PolyglotRequest::outputs`: "Names the block declares as outputs
(`@python -> a, b { … }`), in order") but no example, test or guide page uses
it, so the parser path was never exercised. Root cause (found by reading
`read_output_list` in `crates/crush-frontend/src/parser/lexer.rs`): the
**parenthesized** form `-> (a, b)` loops over comma-separated names, but the
bare (non-parenthesized) branch reads exactly **one** identifier and returns,
leaving `, b` unconsumed; the caller then doesn't find `{` and bails to a
plain `@python` token, producing the "unexpected token in expression"
compile error.

## Success criteria

- [x] `-> a, b` (and `-> a, b, c`) parses on Python/JS/bash blocks (bare form; the parenthesized form keeps working).
- [x] Every output arrives in order on the CVM1 engines (run repros printing each value).
- [x] Tests cover 2+ outputs (lexer, parse, and engine agreement on the two engines that execute polyglot blocks — scheduler VM + PortableVm; FastVM models no polyglot outputs at all, pre-existing gap, see Resolution).

## Technical approach

- Widen the bare branch of `read_output_list` to loop over comma-separated identifiers (same whitespace/`{`-termination convention the single-name form already relies on).
- Keep the parenthesized branch unchanged; malformed lists still bail to `AtIdent` so the normal token stream surfaces the error.
- Add lexer tests for the bare 2- and 3-output forms, and an end-to-end/engine-agreement test running a multi-output block on both CVM1 engines.

## Files to modify

- `crates/crush-frontend/src/parser/lexer.rs` — `read_output_list` bare branch + tests
- engine-agreement test (PortableVm/FastVM) — location per existing `backend_agreement` tests

## Non-goals

- `@bash` output *marshaling* (bash has no value marshaling; `->` on `@bash` stays a loud compile error — the fix is parse-level for bash).
- Comments immediately after the `@lang -> … {` header line (#266, known hostile to the block scanner).

## Resolution

**Root cause:** `read_output_list` in `crates/crush-frontend/src/parser/lexer.rs`
parsed the parenthesized form `-> (a, b)` as a comma-separated loop, but the
**bare** form read exactly one identifier and returned — leaving `, b`
unconsumed. `maybe_consume_polyglot_body` then found `,` instead of `{`,
restored its checkpoint, and bailed to a plain `AtIdent`, so the parser hit
`@python` in expression position: `unexpected token in expression: '@python'`.

**Fix:** the bare branch now loops over comma-separated names (whitespace
skipped before each name and after each `,`), terminated by the block's `{`
exactly like the single-name form. Malformed lists (trailing comma, no name)
still bail to `AtIdent` so the normal token stream surfaces the error.

**Verified (crush 0.11.2, python3 3.14.9 / node v26.13.0 / bash 5.3.20, all
with `--polyglot` and `< /dev/null`):**
- `@python -> a, b { a = 1; b = 2 }` then `print(a + b)` → `3` (the issue's repro)
- `-> a, b, c` → `6`; order check `-> first, second` → `one two`
- `@javascript -> x, y` / `-> x, y, z` → `30` / `6`
- regressions: single `-> a` → `1`; parenthesized `-> (a, b)` → `3`
- `@bash -> a, b` now **parses** and gets its documented loud compile refusal
  (`` `->` output lists are not supported for @bash: it has no value marshaling ``)

**Tests:** lexer (`test_lex_lang_block_with_bare_multi_output`,
`..._bare_three_outputs`), parse (`bare_two_output_list_parses_in_order`,
`bare_three_output_list_parses_in_order`,
`parenthesized_output_list_still_parses_in_order` in `polyglot_braces.rs`),
and engines (`multiple_declared_outputs_arrive_in_order_on_both_engines` in
`polyglot_executor.rs`: 2 and 3 outputs, positionally distinct values, on the
scheduler VM and PortableVm — the two engines that execute polyglot blocks;
FastVM's `ExecLangSite` models no outputs at all, a pre-existing embedder-seam
gap out of scope here). `cargo test -p crush-frontend` all green;
`cargo clippy -p crush-frontend -p crush-lang-sdk --all-targets` — no new
warnings.
