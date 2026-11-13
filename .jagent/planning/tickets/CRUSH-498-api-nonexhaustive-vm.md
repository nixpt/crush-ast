# CRUSH-498 — #[non_exhaustive] on growable public enums: crush-vm and crush-lang-sdk

| Field | Value |
|-------|-------|
| **ID** | CRUSH-498 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#498](https://github.com/nixpt/crush-ast/issues/498) |
| **Size** | M |
| **Dependencies** | CRUSH-484, CRUSH-483, CRUSH-485, CRUSH-464, CRUSH-465 |

## Problem

crush-vm has 38 pub enums and crush-lang-sdk 12; only crush-vm/src/lang_exec.rs:47,83 carry #[non_exhaustive]. Adding a variant to a value, error or capability enum is a breaking change today, contrary to ruling 28. It lands late because the engine tickets above add VmError/FastError variants, which is exactly the growth this protects.

## Evidence (at origin/main 7ecd2515)

- `grep -rn '^\s*pub enum' crates/crush-vm/src` -> 38
- `grep -rn '^\s*pub enum' crates/crush-lang-sdk/src` -> 12
- crates/crush-vm/src/lang_exec.rs:47,83
- ruling 28

## Goal

The core ticket's classification is applied to crush-vm and crush-lang-sdk: growable enums annotated, closed ones allowlisted with reasons. Matches inside the defining crate are unaffected. Wildcard arms added in jit/aot/web/sdk refuse loudly and never pick a default value. No behaviour or performance change on Tier-1 engines.

## Non-goals

- Opcode semantics
- Experimental crates' own enums (jit, debugger, walkers)

## Files

- crates/crush-vm/src
- crates/crush-lang-sdk/src
- scripts/check-non-exhaustive.sh
- docs/api-stability.md
- downstream match sites (wildcard arms only)

## Done when

- `scripts/check-non-exhaustive.sh crush-vm crush-lang-sdk` exits 0
- every new `_ =>` arm outside the defining crate returns an error or refusal (listed in the PR)
- cargo check --workspace --all-targets is green
- cargo test -p crush-vm passes (scheduler VM, PortableVm and FastVM suites) and cargo test -p crush-lang-sdk --test tier1_agreement passes
- cargo +1.100.0 clippy -p crush-vm -p crush-lang-sdk --all-targets -- -D warnings is clean

## Depends on

- CRUSH-484
- CRUSH-483
- CRUSH-485
- CRUSH-464
- CRUSH-465

## Refs

- ruling 28

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
