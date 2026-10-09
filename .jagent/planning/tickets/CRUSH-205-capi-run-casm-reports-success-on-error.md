# CRUSH-205 — C API: `crush_vm_run_casm` returns 0 on runtime errors; the documented embed example prints nothing

| Field | Value |
|-------|-------|
| **ID** | CRUSH-205 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M4 |
| **Assignee** | unassigned |
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

- [ ] `FastYield::Error` → non-zero rc + `last_error` set; success clears `last_error`.
- [ ] Either expose captured output (`crush_vm_output()`) or route `io.print` to stdout.
- [ ] `examples/embed.c` prints what its comment says; `test_embed.c` asserts output.
