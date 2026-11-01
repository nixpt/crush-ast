# CRUSH-229 — Float-to-text exists three times (VM, Rust AOT, C AOT)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-229 |
| **Priority** | P2 |
| **Status** | In review |
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

- [x] Either the generated code comes from one shared spec, or a cross-check test runs a
      fixed set of floats (incl. 0.1+0.2, 1e20, 1e-7, -0.0, NaN, ±inf, subnormals) through
      all three and requires identical text, with the VM as the oracle.
- [x] `stdout_parity.rs` takes expected output from the VM instead of literals.

## Resolution

**Approach.** Rust side: one function, `crush_vm::float_text::crush_float_to_text`
(`crates/crush-vm/src/float_text_core.rs`, std-only). The VM `Display`
(`vm.rs`), `portable_vm::value_to_text`, `io_print` and the `aot_runner` /
`jit_runner` result lines all call it (five more inline copies of the same
`fract() == 0.0` branch were found and removed). Rust AOT embeds that file verbatim
into generated code (`float_text::SOURCE`, the `rust_debug_core` pattern), so the
generated copy cannot drift. Generated C cannot call Rust, so `_float_text` stays a
hand port, guarded by a differential test: the smallest change that makes drift
impossible to miss without building a C generator for one function.

**Real bug found by the test.** C's shortest-digits search (`%.*e` + `strtod`)
rounds an exact tie at the last digit half-to-even (glibc); Rust rounds half-up.
`1000000000000000.25` printed `1000000000000000.2` on C, `...0.3` on the VM and
Rust AOT. Fixed in `_float_text` (`codegen_c.rs`): detect the exact tie and round
half-up with carry.

**Tests.** `crush-aot/tests/float_text_agreement.rs`
(`float_text_is_identical_on_vm_rust_aot_and_c_aot`): ~6000 lines of floats (whole,
tiny, huge, -0.0, NaN, +-inf, subnormals, 17-digit values, exact ties, 1500-step
up/down sweeps), each printed bare and concatenated, byte-identical on VM, Rust AOT
and C AOT. `stdout_parity.rs::floats_print_like_the_vm` now takes its expected text
from the VM. Rule pinned once in `float_text::tests::the_rule`.
