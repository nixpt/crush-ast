# CRUSH-470 — Pin the JIT as opt-in and experimental with a dependency guard (ruling 18/H5)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-470 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#470](https://github.com/nixpt/crush-ast/issues/470) |
| **Size** | S |
| **Dependencies** | CRUSH-454 |

## Problem

Ruling 18/H5 and checkpoint 1 require the JIT to be off by default by 0.13 and labelled experimental (ruling 27). Today it is off by accident of the dependency graph: crates/cli does not depend on crush-jit, but crush-aot depends on it unconditionally (crates/crush-aot/Cargo.toml:43) only to host the jit-runner bin (:57-59), and crush-jit is in default-members (Cargo.toml:44). No test stops a future change from wiring JitEngine into `crush run` or the SDK, and nothing marks the JIT experimental.

## Evidence (at origin/main 7ecd2515)

- crates/crush-aot/Cargo.toml:43, 57-59
- crates/cli/Cargo.toml has no crush-jit / crush-aot dep
- Cargo.toml:35,44
- crates/crush-vm/tests/no_foreign_inference_runtime.rs (dep-graph guard pattern)
- rulings 18/H5, 27; checkpoint 1

## Goal

crush-jit is reachable only behind an explicit opt-in: crush-aot's dep becomes optional under a `jit` feature gating the jit-runner bin, and a dep-graph guard test fails if crush-cli, crush-lang-sdk or crush-vm gain a normal dependency on crush-jit. The crate description, README and docs mark the JIT experimental and opt-in. crush-aot integration tests that construct JitEngine are gated on the feature and CI runs them with it.

## Non-goals

- Deopt to VM (after 1.0)
- Removing crush-jit
- 48-bit overflow behaviour (CRUSH-JIT-INT-RANGE-CHECK)
- Adding a JIT mode to `crush run`

## Files

- crates/crush-aot/Cargo.toml
- crates/crush-jit/Cargo.toml
- crates/crush-jit/README.md
- crates/cli/tests/jit_not_default.rs (new)
- .github/workflows/ci.yml (build and test crush-aot with --features jit)

## Done when

- cargo test -p crush-cli --test jit_not_default passes: via cargo metadata, crush-jit is not in the normal dependency closure of crush-cli, crush-lang-sdk or crush-vm
- cargo tree -p crush-aot -e normal shows no crush-jit without --features jit; cargo check -p crush-aot --features jit --bins builds jit-runner
- cargo test -p crush-aot --features jit still runs the JIT agreement tests (CI step shown green)
- the crush-jit README and Cargo description say 'experimental, opt-in'
- cargo +1.100.0 clippy -p crush-aot --all-targets --features jit -- -D warnings is clean

## Depends on

- CRUSH-454

## Refs

- ruling 18
- H5
- ruling 27
- checkpoint 1

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
