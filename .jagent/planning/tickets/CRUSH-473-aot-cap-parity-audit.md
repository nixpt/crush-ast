# CRUSH-473 — AOT refuses undeclared capabilities like CVM1 (ruling 26)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-473 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#473](https://github.com/nixpt/crush-ast/issues/473) |
| **Size** | M |
| **Dependencies** | CRUSH-452 |

## Problem

CVM1 checks every capability use against the program's manifest before dispatch: check_cap_permitted (crates/crush-vm/src/scheduler.rs:89-101) returns CapNotDeclared when the cap is missing from program.manifest.permissions (crates/casm/src/lib.rs:633) and CapDenied when Quotas::allowed_caps excludes it. Neither AOT backend reads manifest.permissions: codegen.rs emits about 30 CAP_CALL arms inline (e.g. :1243-1252) and codegen_c.rs does the same (:2290-2300). An AOT-compiled program runs any capability whether or not it was declared, which ruling 26 forbids ('AOT enforces the same capability checks as CVM1; Tier-2, but not a sandbox escape'). Only ai_native/dom_native ops are refused today (crush-aot lib.rs:77-135).

## Evidence (at origin/main 7ecd2515)

- crates/crush-vm/src/scheduler.rs:83-101
- crates/casm/src/lib.rs:633
- crates/crush-aot/src/codegen.rs:1243-1252
- crates/crush-aot/src/codegen_c.rs:2290-2300
- crates/crush-aot/src/lib.rs:77-135
- ruling 26; checkpoint 1 (AOT still enforces capability checks)

## Goal

Both AOT backends refuse at compile time, naming the capability and the instruction, any CAP_CALL whose capability is not declared in program.manifest.permissions, mirroring CapNotDeclared. A table-driven test enumerates every CAP_CALL arm the codegens handle and proves each is refused undeclared and runs declared with the same output as FastVM and the scheduler.

## Non-goals

- AOT step/wall quotas (documented Tier-2 gap, checkpoint 1)
- A runtime Quotas::allowed_caps equivalent for AOT binaries (captain question, see notes)
- JIT capability checks (CRUSH-JIT-STR-ORD-GRANT)
- crush-aotc (deleted)

## Files

- crates/crush-aot/src/codegen.rs
- crates/crush-aot/src/codegen_c.rs
- crates/crush-aot/src/lib.rs
- crates/crush-aot/tests/cap_parity.rs (new)

## Done when

- cargo test -p crush-aot --test cap_parity passes: for every CAP_CALL arm in codegen.rs and codegen_c.rs (table-driven; the test fails if an arm has no entry) an undeclared use is refused at compile time on both backends, and the scheduler returns CapNotDeclared for the same program
- for a declared str.ord/conv.ord/io.print sample, AOT Rust and AOT C stdout equals FastVM's and the scheduler's (the C half is skipped with an explicit message only when no cc is found)
- the PR lists every arm with its enforced status
- cargo +1.100.0 clippy -p crush-aot --all-targets -- -D warnings is clean

## Depends on

- CRUSH-452

## Refs

- ruling 26
- #436
- #401

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
