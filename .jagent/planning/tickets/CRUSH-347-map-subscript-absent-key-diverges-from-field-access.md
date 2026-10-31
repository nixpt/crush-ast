# CRUSH-347 — Map subscripting an absent key errors; field access returns null

| Field | Value |
|-------|-------|
| **ID** | CRUSH-347 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M2 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

The two ways to read a map key disagree on a missing key:

- `m.field` (static GET_FIELD on a map) → `null` (confirmed on 0.9.0)
- `m["field"]` / `m[k]` (dynamic subscript, ARR_GET with a string index) →
  `[runtime] type error: expected present map key, got absent key`

So loops that read through computed keys must pre-initialise every key, and
libraries that derive key names at runtime (`"level_" + conv.to_str(i)`)
cannot probe for existence at all. The divergence is invisible in tests that
only use field access.

Found while building `facet`: the parse result map must pre-seed every
declared option key with `null` solely so the dynamic reads in the choices /
required checks don't abort.

## Reproduction

```bash
cat > /tmp/mapkey.crush <<'EOF'
fn main() {
    let m = {"only": 1}
    print(m.only)        // null — field access tolerates absence
    print(m["nope"])     // runtime type error: expected present map key, got absent key
}
EOF
crush-run run /tmp/mapkey.crush
```

## Success criteria

- [ ] Define the contract explicitly and make both forms agree: either
      missing-key reads return `null` in both, or both report a catchable
      error. (Recommendation: return `null` for both, matching field access
      and the JS/Python walkers' expectations — but check walker reliance
      first, since `map_index` is shared.)
- [ ] `has_key(m, k)`-style existence check exists (cap or op) so libraries
      can distinguish "present with null" from "absent" if the null-return
      contract lands.
- [ ] Regression test pins whichever behaviour is chosen for both access
      forms, on PortableVm + scheduler.

## Technical approach

- PortableVm/scheduler: `ARR_GET` → `Value::Map` arm in
  `crates/crush-vm/src/portable_vm.rs` and `scheduler.rs`; the absent-key path
  raises the error today (`map_key_string` + lookup).
- AOT: Rust codegen `map_index` and C codegen `_obj_index` — make them match.
- Check `map_ops.rs` for an existing helper to reuse rather than duplicating.

## Files to modify

- `crates/crush-vm/src/portable_vm.rs`, `crates/crush-vm/src/scheduler.rs` —
  map subscript semantics.
- `crates/crush-aot/src/codegen.rs`, `crates/crush-aot/src/codegen_c.rs` —
  mirror in both AOT backends.
- `crates/crush-vm/src/tests/` — contract tests.

## Non-goals

- Insert-on-read (`m[k]` auto-creating entries).
- Changing map literal parsing (CRUSH-345).
