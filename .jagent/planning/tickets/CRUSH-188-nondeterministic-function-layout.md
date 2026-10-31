# CRUSH-188 — Codegen function order is nondeterministic (`Program.functions` is a `HashMap`)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-188 |
| **Priority** | P1 |
| **Status** | Done |
| **Phase** | M1 |
| **Assignee** | nixp |
| **Dependencies** | none |
| **Estimated effort** | XS |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

Compiling the same file six times produced two different CASM outputs. Function
order follows `HashMap` iteration (`crates/crush-cast/src/lib.rs:38`). This breaks
reproducible builds and caching, and makes layout-dependent bugs (CRUSH-187)
change symptom from run to run.

## Success criteria

- [x] `crushc --emit casm` is byte-identical across runs (use `BTreeMap`/`IndexMap`
      or sort by source order before emission).
- [x] Test compiling a multi-function program twice and comparing bytes.

## Resolution (2026-10-30, `agent/nixp/CRUSH-298`)

`crush-frontend`'s compiler already sorted functions by name before compiling
them (so temporaries and lambdas get stable names). The remaining
nondeterminism was in `crush_lang_sdk::compile::casm_to_vm`
(`crates/crush-lang-sdk/src/compile.rs`), which laid the functions out into the
code buffer by iterating `casm::Program.functions` — a `HashMap` — so the
`crushc --emit casm` disassembly (and the code layout, and the label numbering)
changed from run to run. It now collects the functions and sorts them by name
before emitting.

### Evidence

- `compile::tests::casm_to_vm_lays_out_functions_in_name_order` asserts the
  `.func` order is sorted and that two independent compilations are byte-equal.
- Live: `crushc --emit casm` on a three-function program 12 times gives **1**
  unique output; before the fix the same loop gave **4**.
- `cargo test --workspace --lib --tests`: 2550 passed, 0 failed.
