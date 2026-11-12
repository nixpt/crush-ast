# CRUSH-393 — VM: `DOM_*` opcodes fail closed and a `dom_native` handler's error propagates

| Field | Value |
|-------|-------|
| **ID** | CRUSH-393 |
| **Priority** | P1 |
| **Status** | Done |
| **Phase** | M5 |
| **Assignee** | opencode (dispatched agent) |
| **Dependencies** | none (sibling of CRUSH-234's AI fix) |
| **Estimated effort** | S |
| **Filed by** | opencode — 2026-11-12, from GitHub issue #378 |

> NOTE: GitHub issue numbers and CRUSH ticket numbers are separate namespaces;
> GitHub issue #378 collided with CRUSH-378 (bitwise ops, filed by altair
> 2026-11-06 — unrelated, untouched). This ticket carries the GitHub issue.

## Problem

Sibling of #261 / CRUSH-234 (AI opcodes, fixed in the fail-closed PR). In
`crates/crush-vm/src/scheduler.rs` ~1685-1700 and the PortableVm twin
(`portable_vm.rs` ~2002-2025):

- a DOM opcode (`DOM_QUERY`..`DOM_EVENT`) with no `dom_native.<kind>` handler
  pushed `Value::Null` and kept going — a silently-null DOM query;
- a granted handler's error was dropped via
  `handler.call(vec![]).ok().flatten().unwrap_or(Null)`;
- the handler was always called with no args.

Not touched by the AI wave. Not reachable from `.crush` text (the frontend
lowers `dom_query` to NOP), so the backstop is for precompiled `.cvm1`,
`crush-vm run`, and embedders — same as the AI runtime error's role in
CRUSH-234.

## Evidence

Measured on `main` `d0b7cc03` (v0.11.2) by re-running a hand-built `.cvm1`
containing a single `DOM_QUERY` through `crush-vm run`: exit 0, no output —
the opcode was silently null.

## Success criteria

- [x] An ungranted DOM opcode fails loudly by name: `VmError::CapDenied`
      naming `dom_native.<kind>`, the same text on the scheduler, PortableVm
      and `crush-vm run`, and the same message with no host registry at all.
- [x] A granted handler that errors propagates that error (mapped through
      `HostCapError` like any `CAP_CALL`: timeout → `CapTimeout`, message →
      `UnknownCap`, exit → `Exit`), never a swallowed null.
- [x] Handler-args behaviour unchanged (still `call(vec![])`, arity-checked
      against the handler's spec like the AI dispatch). See Resolution for
      why passing the selector stays out of scope.
- [x] Unit tests at the VM level (`crates/crush-vm/tests/dom_fail_closed.rs`,
      `crush-lang-sdk/tests/dom_native_fail_closed.rs`, `dom_args.rs`) + an
      end-to-end repro: a real `.cvm1` run through `crush-vm run` dies with
      the named refusal on stderr (also covered by the
      `crush_vm_run_names_the_refusal_on_stderr` test).

## Resolution

Done on `agent/opencode/CRUSH-378` (see the commit referencing this ticket).
Mirrors CRUSH-234's `ai_args` pattern exactly:

- New `crates/crush-vm/src/dom_args.rs`: `not_granted(kind)` (one message for
  every engine) and `dispatch(kind, host_caps, deadline_ms)` — ungranted →
  named `CapDenied`; granted → spec arity check + `call_with_deadline` with
  `HostCapError` mapped to VM errors. The scheduler and PortableVm DOM arms
  both call it (`push!(value?)`), so the engines cannot disagree.
- `fastvm/mod.rs`: added `dom_request_denied` — the DOM twin of
  `ai_request_denied` — so FastVM embedders get the same named error instead
  of `resolve_host_request`'s old silent `None`.

Handler args: passing the opcode's string operand (the selector) to DOM
handlers is a real capability gap but NOT small — the stubs declare
`argc: Some(0)`, FastVM's DOM resolution emits a fixed `{ok:true,kind,echo:[]}`
shape, and the AOT codegens (`crush-aot/src/codegen.rs` ~995, `codegen_c.rs`
~1851) carry their own DOM stub shape. All five tiers would have to change
together to keep the differential harness honest. Documented here and left to
CRUSH-252 (the real `DomBridge` provider work), which owns the DOM argument
contract end to end.

## Files touched

- `crates/crush-vm/src/dom_args.rs` (new — shared fail-closed contract)
- `crates/crush-vm/src/lib.rs` (module decl)
- `crates/crush-vm/src/scheduler.rs` (DOM arm rewired)
- `crates/crush-vm/src/portable_vm.rs` (DOM arm rewired)
- `crates/crush-vm/src/fastvm/mod.rs` (`dom_request_denied`)
- `crates/crush-vm/tests/dom_fail_closed.rs` (new — 5 tests incl. the CLI e2e)
- `crates/crush-lang-sdk/tests/dom_native_fail_closed.rs` (new — 3 tests)

Verification: `cargo build -p crush-lang-sdk`; `cargo test -p crush-vm
-p crush-lang-sdk` (1154 passed, 0 failed); `cargo clippy -p crush-vm
-p crush-lang-sdk --all-targets` (no warnings from the new code); manual
repro — `crush-vm run dom.cvm1` exits 1 with the named refusal on stderr,
`crush-run run dom.cvm1` refuses statically naming `dom_native.query`.
