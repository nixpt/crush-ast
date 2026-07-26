# Invariants


## 2026-07-26T18:02:18-05:00

A crate that is a normal Rust library dependency inside this workspace MUST stay crate-type=["lib"]. Every cdylib/staticlib output belongs in its own leaf crate (crush-vm-capi, crush-python, crush-vm-py). A non-rlib crate-type costs the target cargo's -C extra-filename hash, so two build units of it (target graph + host/proc-macro graph) silently overwrite one rlib path and consumers link the wrong one. Enforced deterministically by crates/crush-vm/tests/crate_type_invariant.rs.
