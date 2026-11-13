# CRUSH-475 — Reserve `capability`, `pub` and `mut` (ruling 7): reject `capability NAME perm`, `pub fn` and `let mut` with a reserved-word error

| Field | Value |
|-------|-------|
| **ID** | CRUSH-475 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#475](https://github.com/nixpt/crush-ast/issues/475) |
| **Size** | M |
| **Dependencies** | CRUSH-455 |

## Problem

Ruling 7 reserves `capability`, `pub` and `let mut` with no 1.0 form, but the parser treats each differently and none correctly. (1) parser/mod.rs:594-607 accepts `capability <ident> <ident>` and throws it away under a TODO, so a declared capability compiles and runs as if the line were absent; 8 shipped examples (examples/crush/{sysinfo,colors,calculator,httpclient,benchmark,homepage,greeting,fileman}.crush) open with such a line. (2) `pub` is not a lexer keyword: the parser special-cases Ident "pub" before `async fn` (:646-674) or `fn` (:676-703), so `pub fn` silently means `fn` while `pub` is still a legal variable name; live users are crates/crush-lang-sdk/sbl/sbl_core.crush (3, include_str'd at src/sbl.rs:28) and examples/crush/sbl_core_nanovm.crush (2). (3) `mut` lexes to Token::Mut (lexer.rs:628) but no parser path consumes it, so `let mut x = 1` fails with a generic `expected variable name, found mut`; `let mut` survives in crates/crush-aot/tests/fixtures/sum1k.crush and examples/gpu/q6k_gemv.crush.

## Evidence (at origin/main 7ecd2515)

- crates/crush-frontend/src/parser/mod.rs:594-607, 646-703, 1105-1125
- crates/crush-frontend/src/parser/lexer.rs:168, 183, 625-660
- examples/crush/*.crush: 8 files with a `capability ...` line
- crates/crush-lang-sdk/src/sbl.rs:28; crates/crush-lang-sdk/sbl/sbl_core.crush; examples/crush/sbl_core_nanovm.crush:9,28
- crates/crush-aot/tests/fixtures/sum1k.crush:2-3; examples/gpu/q6k_gemv.crush:48,106,107
- crates/crush-lang-sdk/tests/examples_compile.rs, crates/crush-aot/tests/examples_parity.rs
- ruling 7

## Goal

One located reserved-word ParseError shape for all three: (1) a top-level `capability` statement is rejected; the word stays a keyword and `o.capability` still works. (2) `pub` lexes to a reserved keyword token; `pub fn`, `pub async fn` and `let pub = 1` are rejected; `o.pub` still reads a field (keyword_name gains `pub`); bare `async fn` is unchanged. (3) `let mut x = 1` is rejected with a message saying `let` bindings are already reassignable. In-repo users are fixed in the same PR: the 8 examples drop the capability line, the sbl sources drop `pub`, sum1k.crush drops `mut`, q6k_gemv.crush drops `mut` or gets a header saying it is a GPU-dialect sketch the frontend does not parse. CHANGELOG notes the break, including exosphere's out-of-repo copy of sbl_core.crush.

## Non-goals

- Designing a real capability-declaration, visibility or mutability syntax
- Changing `use @cap`, `export`, or bare `async fn`
- `mut` in other positions (parameters, for bindings): they keep today's error
- `use @mcp` / external imports (CRUSH-RESERVE-MCP-EXTERNAL-IMPORTS)
- tree-sitter grammar (CRUSH-TS-GRAMMAR-FRONTEND-PARITY)
- Editing exosphere's copy of sbl_core.crush

## Files

- crates/crush-frontend/src/parser/mod.rs
- crates/crush-frontend/src/parser/lexer.rs
- crates/crush-frontend/tests/reserved_words.rs (new)
- examples/crush/{sysinfo,colors,calculator,httpclient,benchmark,homepage,greeting,fileman,sbl_core_nanovm}.crush
- crates/crush-lang-sdk/sbl/sbl_core.crush
- crates/crush-aot/tests/fixtures/sum1k.crush
- examples/gpu/q6k_gemv.crush
- CHANGELOG.md

## Done when

- tests: `capability net readonly\nfn main() {}`, `pub fn f() {}`, `pub async fn f() {}`, `let pub = 1` and `let mut x = 1` each yield a located ParseError containing `reserved` and the word
- test: `let o = {}; o.pub = 1; print(o.pub)` and `o.capability` still compile
- `rg -n '^\s*capability\s+\w+' -g '*.crush' .` and `rg -n '\bpub\s+(async\s+)?fn' -g '*.crush' .` return no hits outside crates/tree-sitter-crush; `rg -n 'let mut ' -g '*.crush' .` hits are gone or in a file carrying the GPU-dialect header
- cargo test -p crush-frontend and cargo test -p crush-lang-sdk (sbl tests and examples_compile) pass (one CARGO_TARGET_DIR, df checked first)
- cargo +1.100.0 clippy -p crush-frontend --all-targets -- -D warnings is clean

## Depends on

- CRUSH-455

## Refs

- ruling 7

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
