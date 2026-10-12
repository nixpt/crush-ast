# CRUSH-259 — AOT Rust backend is 3-10x slower and ~70x larger than AOT C

| Field | Value |
|-------|-------|
| **ID** | CRUSH-259 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M2 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-257/258 (the same fixes apply) |
| **Estimated effort** | M |
| **Filed by** | claude (vega) — 2026-10-12 JS-vs-Node benchmark (CRUSH-261) |

## Problem

Same programs, same CASM (CRUSH-261): fib(30) 361 ms (rustc) vs 63 ms (gcc); 20M loop 3348 vs 990/747 ms;
sieve(2M) 1053 vs 278 ms. Each `.so` is ~2.2 MiB (rustc) vs 25-45 KiB (gcc/clang). `gen_rust_source`
(`codegen.rs`) is the same `loop { match __pc { ... } }` shape over a `Vec<RuntimeValue>` stack with
`Rc<RefCell<..>>` arrays/objects, plus a std-linked cdylib per program.

## Success criteria

- [ ] Decide: (a) port CRUSH-257/258 to the Rust emitter (typed locals, structured blocks, no per-op
      `Rc` clone), or (b) make the C backend the default for `crush-aotc`/bozo when a C compiler is available,
      keeping rustc as the fallback. Record the decision.
- [ ] Shrink the Rust `.so`: `panic = "abort"`, `opt-level`, LTO, `-C prefer-dynamic` or a shared runtime crate
      instead of re-embedding the runtime per program; report sizes.
- [ ] CRUSH-261 numbers before/after.
