# CRUSH-zorro-37 — Crush `match` on a struct pattern ignores the struct name (silently wrong)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-37 |
| **Priority** | P0 |
| **Status** | Done (fixed by CRUSH-zorro-43, PR #222 merged 2026-10-15) |
| **Phase** | M9 |
| **Assignee** | vega |
| **Dependencies** | none (the full fix arrives with COSM K1, `docs/design/object-model.md`; this ticket is the interim fix) |
| **Estimated effort** | S |
| **Filed by** | vega — 2026-10-15, found during the object-model survey (#206), reproduced on `origin/main` `c864367` |

## Problem

`Pattern::Struct { name, fields }` is lowered by reading the fields only. The **name is never checked**
(`crates/crush-frontend/src/compiler.rs`, the `Pattern::Struct` arm), so the first arm whose field names match
wins, whatever the value's struct:

```crush
struct P { x: int }
struct Q { x: int }
fn which(v) {
    match v {
        P { x: a } => { return 1; }
        Q { x: b } => { return 2; }
    }
    return 0;
}
print(which(Q { x: 5 }));   // prints 1, expected 2
```

Root cause: a struct value is a plain `Value::Map` at run time. `new_struct` lowers to `NEW_OBJ` and the
struct name is dropped in `casm_to_vm` ("Stubs for unimplemented opcodes"), so there's nothing to compare against.

## Success criteria

- [ ] The program above prints `2`. A value whose struct doesn't match an arm's name never takes that arm.
- [ ] Struct values carry their struct name at run time on every engine that runs them (scheduler VM, portable
      VM, FastVM/JIT, AOT Rust/C), or the engines that can't refuse `match` on struct patterns at compile time
      (exact-or-loud).
- [ ] Maps and object literals (no struct name) never match a named struct pattern.
- [ ] Tests on both VMs: same-field structs, nested struct patterns, a struct pattern vs a map with the same
      keys, a wildcard fallback; AOT exact or loud.
- [ ] The Python walker's keyword class patterns (lowered to `Pattern::Struct`) get the class check too, or stay
      loud until Python classes exist.

## Resolution

Fixed by **CRUSH-zorro-43** (COSM K1.4), the long-term fix rather than the interim tag: a struct value is a Record
of its Kind on the scheduler VM and the portable VM, and a struct pattern tests the Kind (`is_kind` / CVM1
`IS_KIND`) before reading any field. The program above prints `2`:
`crush-lang-sdk/tests/cosm_k1_records.rs::zorro_37_struct_patterns_test_the_kind`.
`struct_patterns_nested_maps_and_wildcards` (same file) covers same-field structs, nested struct patterns, a map
with the same keys (never matches) and a wildcard fallback.

- FastVM, the JIT and AOT Rust/C have no Kinds: `is_kind` is an op they don't know, so a program with a struct
  pattern is refused when it is lowered/compiled (exact-or-loud); crush-aotc refuses it explicitly.
- A struct pattern naming no declared struct is a compile error. That keeps the Python walker's keyword class
  patterns (`case Point(x=…)`) loud until Python classes lower to COSM (K3).

## Technical approach

- Interim (this ticket): keep the struct name on the value, as a hidden tag field or a minimal tagged
  representation, and compare it in the pattern match. FastVM's arena `Object { class_name }` already has a slot
  for it.
- Long term: COSM K1 gives every struct a real Kind, and the pattern match tests the Kind. Make the interim
  representation easy to replace (one helper used by `new_struct` and the pattern match).

## Non-goals

- Enum/ADT patterns, generics (COSM).
