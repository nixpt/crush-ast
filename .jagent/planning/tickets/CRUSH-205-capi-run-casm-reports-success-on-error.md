# CRUSH-205 — C API: `crush_vm_run_casm` returns 0 on runtime errors; the documented embed example prints nothing

| Field | Value |
|-------|-------|
| **ID** | CRUSH-205 |
| **Priority** | P1 |
| **Status** | Done |
| **Phase** | M4 |
| **Assignee** | nixp |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

- C calls to `crush_vm_run_casm` with body `[{"op":"add"},{"op":"ret"}]` return rc=0
  and `last_error` NULL. Same for `jmp` to 99999, `"functions":{}`, and
  `cap_call io.print` without a manifest permission (manifest not enforced).
- `examples/embed.c` prints only the version line, never the advertised 42.
- Cause: `crates/crush-vm-capi/src/lib.rs:~71-76` treats any `Ok(FastYield)` as
  success; `run_fastvm` (`crates/crush-vm/src/vm.rs:~766-790`) passes no caps and a
  `DummyHal` and wraps errors as `Ok(FastYield::Error(..))`. Python `crush.run_casm`
  shows the same thing as the strings `"Error(StackUnderflow)"`.
- `tests/test_embed.c` passes only because of this.

Papercuts:
- `last_error` is never cleared on success.
- README says `--no-default-features` removes the C exports; there are no `cfg` gates.
- The header's usage comment runs `CAP_CALL "io.print"` via `run_asm`, which always
  fails ("capability not declared in manifest").
- The `output` Vec grows every run and is not readable from C.
- Header says version "e.g. 0.2.0".

Related: FastVM host loop is CRUSH-163 (Deferred).

## Success criteria

- [x] `FastYield::Error` → non-zero rc + `last_error` set; success clears `last_error`.
- [x] Either expose captured output (`crush_vm_output()`) or route `io.print` to stdout.
- [x] `examples/embed.c` prints what its comment says; `test_embed.c` asserts output.

## Resolution (2026-10-30, `agent/nixp/CRUSH-300`)

`crush_vm_run_casm` matched any `Ok(FastYield)` as success, so a runtime error
(`add` on an empty stack, a bad jump, an unsupported capability) returned `0`
with `last_error` null. It now reports a run that did not *finish* —
`FastYield::Error`, `BudgetExhausted`, or an unserviceable host request — as
`-2` with a message, and clears `last_error` on success (`crush_vm_run_asm` too).

The bare FastVM has no capabilities, so `cap_call io.print` in an embedded
program went nowhere. `run_casm` now runs through `run_fastvm_with_caps` with a
stdout `io.print` capability (formatting via the shared
`crush_vm::io_print::runtime_value_to_text`, now `pub`), so the example prints
`42`.

### Evidence

- `crates/crush-vm-capi/tests/c_api.rs::run_casm_reports_runtime_errors` — a
  runtime error returns non-zero with `last_error` set; a valid run returns 0
  and clears it (verified RED: the error returned 0).
- `test_c_embed` now also asserts the embedded program's `42` reaches stdout.
- Live: `examples/embed.c` prints `42` then the version; `add`-on-empty returns
  `rc=-2 err=VM execution error: StackUnderflow`.
- `cargo test --workspace --lib --tests`: 2556 passed, 0 failed.

## Follow-ups (not this ticket)

- The FastVM path does not enforce the CASM `manifest` permissions (a program
  can `cap_call io.print` without declaring it); the ticket notes this.
- The README's `--no-default-features` claim still has no `cfg` gates.
- The captured `output` `Vec` still has no C getter.
