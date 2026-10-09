# CRUSH-229 — Float-to-text exists three times (VM, Rust AOT, C AOT)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-229 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-216 |
| **Estimated effort** | S |
| **Filed by** | claude — PR #126 review follow-up, 2026-10-09 |

## Problem

`io.print`'s float rule (shortest round-trip, no exponent, `.0` on whole numbers,
`NaN`/`inf`) is implemented in `crush-vm/src/io_print.rs`, in `crush_float_to_text`
(`crush-aot/src/codegen.rs`, generated Rust) and in `_float_text`
(`crush-aot/src/codegen_c.rs`, generated C). CLAUDE.md: "One capability, one shared
implementation, every backend calls into it" (CRUSH-114). If the VM's rule changes,
the copies drift silently; `stdout_parity.rs` pins hand-written expected strings rather
than the VM's output. Review:
https://github.com/nixpt/crush-ast/pull/126#discussion_r4226980262

## Success criteria

- [ ] Either the generated code comes from one shared spec, or a cross-check test runs a
      fixed set of floats (incl. 0.1+0.2, 1e20, 1e-7, -0.0, NaN, ±inf, subnormals) through
      all three and requires identical text, with the VM as the oracle.
- [ ] `stdout_parity.rs` takes expected output from the VM instead of literals.
