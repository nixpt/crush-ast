# CRUSH-zorro-37 — Crush `match` on a struct pattern ignores the struct name (silently wrong)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-37 |
| **Priority** | P0 |
| **Status** | Backlog |
| **Phase** | M9 |
| **Assignee** | unassigned |
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

## Technical approach

- Interim (this ticket): keep the struct name on the value, as a hidden tag field or a minimal tagged
  representation, and compare it in the pattern match. FastVM's arena `Object { class_name }` already has a slot
  for it.
- Long term: COSM K1 gives every struct a real Kind, and the pattern match tests the Kind. Make the interim
  representation easy to replace (one helper used by `new_struct` and the pattern match).

## Non-goals

- Enum/ADT patterns, generics (COSM).
