# CRUSH-495 — crush doctor states the effective polyglot mode (sandboxed / unsandboxed-only / unavailable)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-495 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#495](https://github.com/nixpt/crush-ast/issues/495) |
| **Size** | S |
| **Dependencies** | CRUSH-459, CRUSH-460 |

## Problem

`crush doctor` prints `sandboxed-polyglot=` as a build flag (crates/crush-lang-sdk/src/doctor.rs:55-64, 138-139) and marks bwrap required only in a sandboxed build (doctor.rs:93-100; test at 296-317). It never states what `--polyglot` will actually do on this host. Once the sandbox default and --polyglot-unsandboxed land, a user cannot tell from doctor whether --polyglot will run sandboxed or be refused (CRUSH-236 criterion 4).

## Evidence (at origin/main 7ecd2515)

- crates/crush-lang-sdk/src/doctor.rs:55-64,93-100,138-139,296-317
- crates/crush-lang-sdk/src/bin/crush.rs:188-215

## Goal

Report gets a `polyglot_mode` field, printed as a line and present in --json: `sandboxed` (sandbox build, isolation available), `unsandboxed-only` (no sandbox in this build or host: --polyglot is refused and only --polyglot-unsandboxed runs) or `unavailable` (no interpreters found), plus a one-line reason. The mode uses the same isolation check as CRUSH-POLYGLOT-FAIL-CLOSED, so doctor and the runtime cannot disagree. The exit-code rule stays. The docs polyglot page lists the three modes. Wording stays backend-neutral.

## Non-goals

- Changing the sandbox backend
- Checks for tools other than bwrap and the interpreters
- New exit-code semantics

## Files

- crates/crush-lang-sdk/src/doctor.rs
- crates/crush-lang-sdk/src/bin/crush.rs
- docs polyglot page

## Done when

- cargo test -p crush-lang-sdk --lib doctor: a fake-PATH test with interpreters but no bwrap asserts polyglot_mode == unsandboxed-only (any build); a test with a fake bwrap asserts `sandboxed` when crush_vm::SANDBOXED_POLYGLOT is true and `unsandboxed-only` otherwise; a test with no interpreters asserts `unavailable`
- `crush doctor --json` output contains a non-null polyglot_mode (integration test parses the JSON)
- cargo +1.100.0 clippy -p crush-lang-sdk --all-targets -- -D warnings clean

## Depends on

- CRUSH-459
- CRUSH-460

## Refs

- CRUSH-236

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
