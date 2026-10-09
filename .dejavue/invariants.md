# Invariants


## 2026-07-26T18:02:18-05:00

A crate that is a normal Rust library dependency inside this workspace MUST stay crate-type=["lib"]. Every cdylib/staticlib output belongs in its own leaf crate (crush-vm-capi, crush-python, crush-vm-py). A non-rlib crate-type costs the target cargo's -C extra-filename hash, so two build units of it (target graph + host/proc-macro graph) silently overwrite one rlib path and consumers link the wrong one. Enforced deterministically by crates/crush-vm/tests/crate_type_invariant.rs.


## 2026-10-09T03:42:00+00:00

The crush-frontend optimizer must not change what a program observably does: it may not change how many times a subexpression is evaluated, drop a call or other side effect, or change a value's type. Rewrites that need type or purity information are not allowed until the optimizer has that information. Check: every compilable examples/crush/*.crush prints the same output with and without crushc -O (CRUSH-189; a CI test for this is not yet in place).
