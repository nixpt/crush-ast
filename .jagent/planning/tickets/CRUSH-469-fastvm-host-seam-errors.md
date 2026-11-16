# CRUSH-469 — FastVM host seam distinguishes allowlist refusal from handler failure

| Field | Value |
|-------|-------|
| **ID** | CRUSH-469 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#469](https://github.com/nixpt/crush-ast/issues/469) |
| **Size** | M |
| **Dependencies** | none |

## Problem

resolve_host_request_with_quotas collapses the distinct VmError::CapDenied from check_cap_permitted, and any handler error, to None via .ok()? on both the AI and DOM paths. ai_request_denied/dom_request_denied take no Quotas, so an allowlist-refused op reads as 'not denied'. An embedder following the documented pattern treats an allowlist refusal as a handler failure, while scheduler and PortableVm raise a named CapDenied. The path fails closed, so this is a parity and diagnosability gap, not a bypass. The quota-less resolve_host_request also ignores the allowlist and is not deprecated.

## Evidence (at origin/main 7ecd2515)

- crates/crush-vm/src/fastvm/mod.rs:265,290, 324,337, 394,402
- issue #433

## Goal

A new seam function returning Result<RuntimeValue, VmError> that keeps CapDenied distinct from handler failure. Both old resolve functions carry #[deprecated] pointing at it. Tests cover AI and DOM, each with allowlist-refused, handler-failed and success, plus the wall-clock deadline path.

## Non-goals

- Wiring FastVM host yields into a consumer (CRUSH-163, deferred)
- New host kinds
- Removing the deprecated functions (ruling 34)

## Files

- crates/crush-vm/src/fastvm/mod.rs
- crates/crush-vm/src/dom_args.rs
- crates/crush-vm/src/ai_args.rs

## Done when

- cargo test -p crush-vm host_seam runs 6 cases (AI and DOM x refused / failed / ok), each asserting the distinct outcome, plus 1 deadline case
- resolve_host_request and resolve_host_request_with_quotas carry #[deprecated] and in-tree callers are migrated (no deprecation warnings in the crate)
- cargo +1.100.0 clippy -p crush-vm --all-targets -- -D warnings is clean

## Depends on

- none

## Refs

- #433
- CRUSH-163

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
