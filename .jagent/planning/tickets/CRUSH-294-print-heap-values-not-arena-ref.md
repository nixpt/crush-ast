# CRUSH-294 — FastVM/JIT print heap values, not the arena ref (GitHub #267)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-294 |
| **Priority** | P0 |
| **Status** | Done |
| **Phase** | M1 |
| **Assignee** | nixp |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | nixp — 2026-10-30, from GitHub #267 |

## Problem

On FastVM and the JIT, `io.print` of any heap value (array, map, struct, tuple,
…) printed the **arena reference** (`@259`) instead of the value, and the run
finished successfully. Every other engine prints the value.

`crates/crush-vm/src/io_print.rs::runtime_value_to_text` resolved a `Ref` only
when it pointed at an `Object::Str`; every other object kind fell through to
`format!("@{idx}")`. `PrintCap` (the `io.print` capability both fast engines
call) formats its arguments through that function.

## Success criteria

- [x] Every value kind shared by the engines prints its value, formatted exactly
      like the VM's `impl Display for Value`.
- [x] An all-engine print test for arrays, maps, nested values and structs.

## Resolution (2026-10-30, `agent/nixp/CRUSH-294`)

`runtime_value_to_text` now resolves the arena object and renders it the way the
VM's `Display` does: arrays `[a, b]`, tuples `(a, b)`, vectors `Vector[…]`, sets
`Set{…}`, lists `List[…]`, maps and structs `{k: v, …}`, bytes `<N bytes>`,
handles `<handle N>`, tagged `tag(v)`, results `ok(v)` / `err(v)`; strings stay
raw (unquoted), including inside a container. One formatter, both fast engines.

### Evidence

- `crates/crush-aot/tests/stdout_parity.rs::heap_values_print_their_value_not_the_arena_ref`
  compares printed stdout on the VM, FastVM, JIT, AOT Rust and AOT C for an
  array, a map, nested arrays, a struct and a scalar. Verified RED before the
  fix (`fastvm: "@259\n"` vs `"[1, 2, 3]\n"`) and GREEN after.
- `crates/crush-vm/src/io_print.rs::tests::runtime_value_to_text_renders_heap_values_like_the_vm`.
- Live: `crush-run` prints `[1, 2, 3]`, `{a: 1}`, `[[1], [2, 3]]`, `{x: 5}`, `42`.
- `cargo test --workspace --lib --tests`: 2541 passed, 0 failed.
